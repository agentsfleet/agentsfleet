const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");
const hx_mod = @import("../hx.zig");

const log = std.log.scoped(.http);

const sql_get_agent =
    \\SELECT agent_id, name, status, workspace_id, trust_level, trust_streak_runs, created_at, updated_at
    \\FROM agent_profiles WHERE agent_id = $1
;

fn innerGetAgent(hx: hx_mod.Hx, req: *httpz.Request, agent_id: []const u8) void {
    _ = req;

    if (!id_format.isSupportedAgentId(agent_id)) {
        hx.fail(error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid agent_id format");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var q = conn.query(sql_get_agent, .{agent_id}) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer q.deinit();

    const row = q.next() catch null orelse {
        hx.fail(error_codes.ERR_AGENT_NOT_FOUND, "Agent not found");
        return;
    };

    const rid = row.get([]u8, 0) catch "?";
    const name = row.get([]u8, 1) catch "?";
    const status = row.get([]u8, 2) catch "?";
    const workspace_id = row.get([]u8, 3) catch "?";
    const trust_level = row.get([]u8, 4) catch "UNEARNED";
    const trust_streak_runs = row.get(i32, 5) catch 0;
    const created_at = row.get(i64, 6) catch 0;
    const updated_at = row.get(i64, 7) catch 0;

    q.drain() catch |err| obs_log.logWarnErr(.http, err, "agent.query_drain_fail agent_id={s}", .{agent_id});

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(error_codes.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }
    if (!common.requireRole(hx.res, hx.req_id, hx.principal, .user)) return;

    // Pipeline v1 removed — improvement stalled warnings are no longer generated.
    const improvement_stalled_warning = false;

    log.debug("agent.get agent_id={s} status={s}", .{ agent_id, status });

    hx.ok(.ok, .{
        .agent_id = rid,
        .name = name,
        .status = status,
        .workspace_id = workspace_id,
        .trust_level = trust_level,
        .trust_streak_runs = trust_streak_runs,
        .improvement_stalled_warning = improvement_stalled_warning,
        .created_at = created_at,
        .updated_at = updated_at,
        .request_id = hx.req_id,
    });
}

pub const handleGetAgent = hx_mod.authenticatedWithParam(innerGetAgent);

test "integration: get agent returns 404 for unknown agent_id" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = try db_ctx.conn.query(sql_get_agent, .{"0195b4ba-8d3a-7f13-8abc-000000000000"});
    defer q.deinit();
    const row = try q.next();
    try std.testing.expect(row == null);
}
