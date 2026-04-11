const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");
const proposals = @import("../../../pipeline/scoring_mod/proposals.zig");

const log = std.log.scoped(.http);

const sql_get_agent =
    \\SELECT agent_id, name, status, workspace_id, trust_level, trust_streak_runs, created_at, updated_at
    \\FROM agent_profiles WHERE agent_id = $1
;

pub fn handleGetAgent(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, agent_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

    if (!id_format.isSupportedAgentId(agent_id)) {
        common.errorResponse(res, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid agent_id format", req_id);
        return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var q = conn.query(sql_get_agent, .{agent_id}) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer q.deinit();

    const row = q.next() catch null orelse {
        common.errorResponse(res, error_codes.ERR_AGENT_NOT_FOUND, "Agent not found", req_id);
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

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }
    if (!common.requireRole(res, req_id, principal, .user)) return;

    const improvement_stalled_warning = proposals.hasImprovementStalledWarning(conn, agent_id) catch false;

    log.debug("agent.get agent_id={s} status={s}", .{ agent_id, status });

    common.writeJson(res, .ok, .{
        .agent_id = rid,
        .name = name,
        .status = status,
        .workspace_id = workspace_id,
        .trust_level = trust_level,
        .trust_streak_runs = trust_streak_runs,
        .improvement_stalled_warning = improvement_stalled_warning,
        .created_at = created_at,
        .updated_at = updated_at,
        .request_id = req_id,
    });
}

test "integration: get agent returns 404 for unknown agent_id" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = try db_ctx.conn.query(sql_get_agent, .{"0195b4ba-8d3a-7f13-8abc-000000000000"});
    defer q.deinit();
    const row = try q.next();
    try std.testing.expect(row == null);
}
