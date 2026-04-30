const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const policy = @import("../../../state/policy.zig");
const obs_log = @import("../../../observability/logging.zig");
const error_codes = @import("../../../errors/error_registry.zig");
const common = @import("../common.zig");
const workspace_guards = @import("../../workspace_guards.zig");
const hx_mod = @import("../hx.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

/// PATCH /v1/workspaces/{workspace_id} — partial update. Today the only
/// patchable surface is the pause/unpause toggle (body shape carries
/// `{pause, reason, version}`); future fields land here too. Optimistic
/// concurrency via `version` — a stale client gets WORKSPACE_NOT_FOUND.
pub fn innerPatchWorkspace(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        pause: bool,
        reason: []const u8,
        version: i64,
    };

    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    policy.recordPolicyEvent(conn, workspace_id, null, .sensitive, .allow, "m1.pause_workspace", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "workspace.policy_event_insert_fail workspace_id={s}", .{workspace_id});
    };

    const now_ms = std.time.milliTimestamp();
    var upd = PgQuery.from(conn.query(
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
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer upd.deinit();

    const row = upd.next() catch null orelse {
        hx.fail(error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found or version conflict");
        return;
    };

    const new_version = row.get(i64, 0) catch 0;
    log.info("workspace.pause_updated pause={} workspace_id={s}", .{ parsed.value.pause, workspace_id });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .paused = parsed.value.pause,
        .version = new_version,
        .request_id = hx.req_id,
    });
}
