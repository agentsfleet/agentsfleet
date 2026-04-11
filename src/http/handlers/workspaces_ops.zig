const std = @import("std");
const httpz = @import("httpz");
const policy = @import("../../state/policy.zig");
const obs_log = @import("../../observability/logging.zig");
const error_codes = @import("../../errors/codes.zig");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub fn handlePauseWorkspace(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        pause: bool,
        reason: []const u8,
        version: i64,
    };

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const actor = principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    policy.recordPolicyEvent(conn, workspace_id, null, .sensitive, .allow, "m1.pause_workspace", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "workspace.policy_event_insert_fail workspace_id={s}", .{workspace_id});
    };

    const now_ms = std.time.milliTimestamp();
    var upd = conn.query(
        \\UPDATE workspaces
        \\SET paused = $1, paused_reason = $2, version = version + 1, updated_at = $3
        \\WHERE workspace_id = $4 AND version = $5
        \\RETURNING version
    , .{
        parsed.value.pause,
        if (parsed.value.pause) parsed.value.reason else null,
        now_ms,
        workspace_id,
        parsed.value.version,
    }) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer upd.deinit();

    const row = upd.next() catch null orelse {
        common.errorResponse(res, .conflict, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found or version conflict", req_id);
        return;
    };

    const new_version = row.get(i64, 0) catch 0;
    upd.drain() catch {};
    log.info("workspace.pause_updated pause={} workspace_id={s}", .{ parsed.value.pause, workspace_id });

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .paused = parsed.value.pause,
        .version = new_version,
        .request_id = req_id,
    });
}

/// M10_001: Pipeline v1 removed — specs table dropped. Stub returns 410.
pub fn handleSyncSpecs(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    _ = req;
    _ = workspace_id;
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);
    common.errorResponse(res, .gone, error_codes.ERR_PIPELINE_V1_REMOVED, "Pipeline v1 removed — spec sync is no longer available", req_id);
}
