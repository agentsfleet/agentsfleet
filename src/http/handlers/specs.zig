const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");

const log = std.log.scoped(.http);

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
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "workspace_id query param required", req_id);
        return;
    };
    const workspace_id = qs.get("workspace_id") orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "workspace_id query param required", req_id);
        return;
    };
    const wid = workspace_id;
    if (!common.requireUuidV7Id(res, req_id, wid, "workspace_id")) return;

    const limit_str = qs.get("limit");
    const limit: i32 = if (limit_str) |ls|
        std.fmt.parseInt(i32, ls, 10) catch 50
    else
        50;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, wid)) {
        common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    log.debug("spec.list workspace_id={s} limit={d}", .{ wid, limit });

    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result = conn.query(
        \\SELECT spec_id, file_path, title, status, created_at, updated_at
        \\FROM specs WHERE workspace_id = $1
        \\ORDER BY created_at DESC LIMIT $2
    , .{ wid, limit }) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer result.deinit();

    var specs: std.ArrayList(std.json.Value) = .{};

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
        specs.append(alloc, .{ .object = obj }) catch continue;
    }

    log.info("spec.listed workspace_id={s} count={d}", .{ wid, specs.items.len });

    common.writeJson(res, .ok, .{
        .specs = specs.items,
        .total = specs.items.len,
        .request_id = req_id,
    });
}
