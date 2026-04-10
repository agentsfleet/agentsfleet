const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const obs_log = @import("../../observability/logging.zig");

const log = std.log.scoped(.http);

const sql_specs_first_page =
    \\SELECT spec_id, file_path, title, status, created_at, updated_at
    \\FROM specs WHERE workspace_id = $1
    \\ORDER BY spec_id DESC LIMIT $2
;

const sql_specs_first_page_with_status =
    \\SELECT spec_id, file_path, title, status, created_at, updated_at
    \\FROM specs WHERE workspace_id = $1 AND status = $2
    \\ORDER BY spec_id DESC LIMIT $3
;

const sql_specs_after_cursor =
    \\SELECT spec_id, file_path, title, status, created_at, updated_at
    \\FROM specs WHERE workspace_id = $1 AND spec_id < $2
    \\ORDER BY spec_id DESC LIMIT $3
;

const sql_specs_after_cursor_with_status =
    \\SELECT spec_id, file_path, title, status, created_at, updated_at
    \\FROM specs WHERE workspace_id = $1 AND status = $2 AND spec_id < $3
    \\ORDER BY spec_id DESC LIMIT $4
;

pub fn handleListSpecs(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

    const qs = req.query() catch {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "workspace_id query param required", req_id);
        return;
    };
    const workspace_id = qs.get("workspace_id") orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "workspace_id query param required", req_id);
        return;
    };
    const wid = workspace_id;
    if (!common.requireUuidV7Id(res, req_id, wid, "workspace_id")) return;

    const pg = common.parsePaginationParams(qs.get("limit"), qs.get("starting_after"));
    const status_filter: ?[]const u8 = qs.get("status");

    if (pg.starting_after) |cursor| {
        if (!common.requireUuidV7Id(res, req_id, cursor, "starting_after")) return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, wid)) {
        common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const fetch_limit = pg.limit + 1;

    log.debug("spec.list workspace_id={s} limit={d} cursor={?s} status={?s}", .{ wid, pg.limit, pg.starting_after, status_filter });

    var result = if (pg.starting_after) |cursor|
        if (status_filter) |sf|
            conn.query(sql_specs_after_cursor_with_status, .{ wid, sf, cursor, fetch_limit }) catch {
                common.internalDbError(res, req_id);
                return;
            }
        else
            conn.query(sql_specs_after_cursor, .{ wid, cursor, fetch_limit }) catch {
                common.internalDbError(res, req_id);
                return;
            }
    else if (status_filter) |sf|
        conn.query(sql_specs_first_page_with_status, .{ wid, sf, fetch_limit }) catch {
            common.internalDbError(res, req_id);
            return;
        }
    else
        conn.query(sql_specs_first_page, .{ wid, fetch_limit }) catch {
            common.internalDbError(res, req_id);
            return;
        };
    defer result.deinit();

    var data: std.ArrayList(std.json.Value) = .{};

    while (result.next() catch null) |row| {
        const spec_id = row.get([]u8, 0) catch continue;
        const file_path = row.get([]u8, 1) catch continue;
        const title = row.get([]u8, 2) catch continue;
        const status = row.get([]u8, 3) catch continue;
        const created_at = row.get(i64, 4) catch 0;
        const updated_at = row.get(i64, 5) catch 0;

        var obj = std.json.ObjectMap.init(alloc);
        obj.put("spec_id", .{ .string = spec_id }) catch continue;
        obj.put("file_path", .{ .string = file_path }) catch continue;
        obj.put("title", .{ .string = title }) catch continue;
        obj.put("status", .{ .string = status }) catch continue;
        obj.put("created_at", .{ .integer = created_at }) catch continue;
        obj.put("updated_at", .{ .integer = updated_at }) catch continue;
        data.append(alloc, .{ .object = obj }) catch continue;
    }
    result.drain() catch |err| obs_log.logWarnErr(.http, err, "spec.list_drain_fail workspace_id={s}", .{wid});

    const pag = common.derivePaginationResult(data.items, pg.limit, "spec_id");

    log.info("spec.listed workspace_id={s} count={d} has_more={}", .{ wid, pag.data.len, pag.has_more });

    common.writeJson(res, .ok, .{
        .data = pag.data,
        .has_more = pag.has_more,
        .next_cursor = pag.next_cursor,
        .request_id = req_id,
    });
}
