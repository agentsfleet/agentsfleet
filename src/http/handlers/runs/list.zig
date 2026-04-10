const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const error_codes = @import("../../../errors/codes.zig");
const obs_log = @import("../../../observability/logging.zig");

const log = std.log.scoped(.http);

const sql_runs_first_page_ws =
    \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
    \\       requested_by, pr_url, created_at, updated_at
    \\FROM runs WHERE workspace_id = $1
    \\ORDER BY run_id DESC LIMIT $2
;

const sql_runs_after_cursor_ws =
    \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
    \\       requested_by, pr_url, created_at, updated_at
    \\FROM runs WHERE workspace_id = $1 AND run_id < $2
    \\ORDER BY run_id DESC LIMIT $3
;

const sql_runs_first_page_all =
    \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
    \\       requested_by, pr_url, created_at, updated_at
    \\FROM runs
    \\ORDER BY run_id DESC LIMIT $1
;

const sql_runs_after_cursor_all =
    \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
    \\       requested_by, pr_url, created_at, updated_at
    \\FROM runs WHERE run_id < $1
    \\ORDER BY run_id DESC LIMIT $2
;

pub fn handleListRuns(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

    const qs = req.query() catch null;
    const workspace_id = if (qs) |q| q.get("workspace_id") else null;

    if (workspace_id) |wid| {
        if (!common.requireUuidV7Id(res, req_id, wid, "workspace_id")) return;
    }

    const pg = common.parsePaginationParams(
        if (qs) |q| q.get("limit") else null,
        if (qs) |q| q.get("starting_after") else null,
    );

    if (pg.starting_after) |cursor| {
        if (!common.requireUuidV7Id(res, req_id, cursor, "starting_after")) return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (workspace_id) |wid| {
        if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, wid)) {
            common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
            return;
        }
    }

    const fetch_limit = pg.limit + 1;

    log.debug("run.list workspace_id={?s} limit={d} cursor={?s}", .{ workspace_id, pg.limit, pg.starting_after });

    var runs: std.ArrayList(std.json.Value) = .{};

    if (workspace_id) |wid| {
        var result = if (pg.starting_after) |cursor|
            conn.query(sql_runs_after_cursor_ws, .{ wid, cursor, fetch_limit }) catch {
                common.internalDbError(res, req_id);
                return;
            }
        else
            conn.query(sql_runs_first_page_ws, .{ wid, fetch_limit }) catch {
                common.internalDbError(res, req_id);
                return;
            };
        defer result.deinit();
        appendRows(alloc, &runs, &result);
        result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.list_drain_fail workspace_id={s}", .{wid});
    } else {
        var result = if (pg.starting_after) |cursor|
            conn.query(sql_runs_after_cursor_all, .{ cursor, fetch_limit }) catch {
                common.internalDbError(res, req_id);
                return;
            }
        else
            conn.query(sql_runs_first_page_all, .{fetch_limit}) catch {
                common.internalDbError(res, req_id);
                return;
            };
        defer result.deinit();
        appendRows(alloc, &runs, &result);
        result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.list_drain_fail_all", .{});
    }

    const pag = common.derivePaginationResult(runs.items, pg.limit, "run_id");

    log.info("run.listed workspace_id={?s} count={d} has_more={}", .{ workspace_id, pag.data.len, pag.has_more });

    common.writeJson(res, .ok, .{
        .data = pag.data,
        .has_more = pag.has_more,
        .next_cursor = pag.next_cursor,
        .request_id = req_id,
    });
}

fn appendRows(alloc: std.mem.Allocator, runs: *std.ArrayList(std.json.Value), result: anytype) void {
    while (result.*.next() catch null) |row| {
        const run_id = row.get([]u8, 0) catch continue;
        const workspace_id = row.get([]u8, 1) catch continue;
        const spec_id = row.get([]u8, 2) catch continue;
        const state = row.get([]u8, 3) catch continue;
        const attempt = row.get(i32, 4) catch 1;
        const mode = row.get([]u8, 5) catch continue;
        const requested_by = row.get([]u8, 6) catch continue;
        const pr_url = row.get(?[]u8, 7) catch null;
        const created_at = row.get(i64, 8) catch 0;
        const updated_at = row.get(i64, 9) catch 0;

        var obj = std.json.ObjectMap.init(alloc);
        obj.put("run_id", .{ .string = run_id }) catch continue;
        obj.put("workspace_id", .{ .string = workspace_id }) catch continue;
        obj.put("spec_id", .{ .string = spec_id }) catch continue;
        obj.put("state", .{ .string = state }) catch continue;
        obj.put("attempt", .{ .integer = @as(i64, attempt) }) catch continue;
        obj.put("mode", .{ .string = mode }) catch continue;
        obj.put("requested_by", .{ .string = requested_by }) catch continue;
        if (pr_url) |url| {
            obj.put("pr_url", .{ .string = url }) catch {};
        }
        obj.put("created_at", .{ .integer = created_at }) catch continue;
        obj.put("updated_at", .{ .integer = updated_at }) catch continue;
        runs.append(alloc, .{ .object = obj }) catch continue;
    }
}
