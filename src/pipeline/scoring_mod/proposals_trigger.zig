const std = @import("std");
const pg = @import("pg");
const shared = @import("proposals_shared.zig");

pub fn detectRollingWindowTrigger(conn: *pg.Conn, agent_id: []const u8) !?shared.RollingTrigger {
    var q = try conn.query(
        \\SELECT score
        \\FROM agent_run_scores
        \\WHERE agent_id = $1
        \\ORDER BY scored_at DESC, score_id DESC
        \\LIMIT 10
    , .{agent_id});
    defer q.deinit();

    var scores: [10]i32 = undefined;
    var count: usize = 0;
    while (count < scores.len) {
        const row = (try q.next()) orelse break;
        scores[count] = try row.get(i32, 0);
        count += 1;
    }
    if (count == scores.len) q.drain() catch {};

    if (count < 5) return null;

    const current_sum = sumScores(scores[0..5]);
    if (current_sum < 300) return .{ .reason = .sustained_low_score };
    if (count < 10) return null;

    const previous_sum = sumScores(scores[5..10]);
    if (current_sum < previous_sum) return .{ .reason = .declining_score };
    return null;
}

pub fn loadActiveConfigContext(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    agent_id: []const u8,
) !?shared.ActiveConfigContext {
    const sql = if (agent_id.len == 0)
        \\SELECT COALESCE(MAX(p.trust_level), 'UNEARNED'), active.config_version_id
        \\FROM workspace_active_config active
        \\LEFT JOIN agent_profiles p ON p.workspace_id = active.workspace_id
        \\WHERE active.workspace_id = $1
        \\GROUP BY active.config_version_id
        \\LIMIT 1
    else
        \\SELECT p.trust_level, active.config_version_id
        \\FROM agent_profiles p
        \\JOIN workspace_active_config active ON active.workspace_id = p.workspace_id
        \\WHERE p.workspace_id = $1 AND p.agent_id = $2
        \\LIMIT 1
    ;
    var q = if (agent_id.len == 0)
        try conn.query(sql, .{workspace_id})
    else
        try conn.query(sql, .{ workspace_id, agent_id });

    const row = (try q.next()) orelse {
        q.deinit();
        return null;
    };
    const result = shared.ActiveConfigContext{
        .trust_level = try alloc.dupe(u8, try row.get([]const u8, 0)),
        .config_version_id = try alloc.dupe(u8, try row.get([]const u8, 1)),
    };
    q.drain() catch {};
    q.deinit();
    return result;
}

pub fn hasPendingOrReadyProposal(conn: *pg.Conn, agent_id: []const u8, config_version_id: []const u8) !bool {
    var q = try conn.query(
        \\SELECT 1
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
        \\  AND config_version_id = $2
        \\  AND generation_status IN ($3, $4)
        \\LIMIT 1
    , .{ agent_id, config_version_id, shared.GENERATION_STATUS_PENDING, shared.GENERATION_STATUS_READY });

    const found = (try q.next()) != null;
    if (found) q.drain() catch {};
    q.deinit();
    return found;
}

pub fn rejectGenerationProposal(conn: *pg.Conn, proposal_id: []const u8, rejection_reason: []const u8) !void {
    _ = try conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET generation_status = $2,
        \\    status = $3,
        \\    rejection_reason = $4,
        \\    updated_at = $5
        \\WHERE proposal_id = $1
    , .{
        proposal_id,
        shared.GENERATION_STATUS_REJECTED,
        shared.STATUS_REJECTED,
        rejection_reason,
        std.time.milliTimestamp(),
    });
}

fn sumScores(scores: []const i32) i32 {
    var total: i32 = 0;
    for (scores) |score| total += score;
    return total;
}
