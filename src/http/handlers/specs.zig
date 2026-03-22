const std = @import("std");
const zap = @import("zap");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");

const log = std.log.scoped(.http);

pub fn handleListSpecs(ctx: *common.Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    const workspace_id = r.getParamStr(alloc, "workspace_id") catch null orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "workspace_id query param required", req_id);
        return;
    };
    defer alloc.free(workspace_id);
    const wid = workspace_id;
    if (!common.requireUuidV7Id(r, req_id, wid, "workspace_id")) return;

    const limit_str = r.getParamStr(alloc, "limit") catch null;
    defer if (limit_str) |ls| alloc.free(ls);
    const limit: i32 = if (limit_str) |ls|
        std.fmt.parseInt(i32, ls, 10) catch 50
    else
        50;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, wid)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    log.debug("spec.list workspace_id={s} limit={d}", .{ wid, limit });

    // check-pg-drain: ok — full while loop exhausts all rows, natural drain
    var result = conn.query(
        \\SELECT spec_id, file_path, title, status, created_at, updated_at
        \\FROM specs WHERE workspace_id = $1
        \\ORDER BY created_at DESC LIMIT $2
    , .{ wid, limit }) catch {
        common.internalDbError(r, req_id);
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

    common.writeJson(r, .ok, .{
        .specs = specs.items,
        .total = specs.items.len,
        .request_id = req_id,
    });
}
