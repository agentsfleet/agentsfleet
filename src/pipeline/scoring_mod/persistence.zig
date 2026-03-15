const std = @import("std");
const pg = @import("pg");
const obs_log = @import("../../observability/logging.zig");
const id_format = @import("../../types/id_format.zig");
const types = @import("types.zig");
const math = @import("math.zig");

pub fn queryLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) ?types.LatencyBaseline {
    var q = conn.query(
        \\SELECT p50_seconds, p95_seconds, sample_count
        \\FROM workspace_latency_baseline
        \\WHERE workspace_id = $1
    , .{workspace_id}) catch return null;
    defer q.deinit();

    const row = (q.next() catch return null) orelse return null;
    return types.LatencyBaseline{
        .p50_seconds = @intCast(@max(row.get(i64, 0) catch return null, 0)),
        .p95_seconds = @intCast(@max(row.get(i64, 1) catch return null, 0)),
        .sample_count = @intCast(@max(row.get(i32, 2) catch return null, 0)),
    };
}

pub fn updateLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) void {
    const now_ms = std.time.milliTimestamp();
    var q = conn.query(
        \\WITH completed_runs AS (
        \\    SELECT COALESCE(SUM(ul.agent_seconds), 0) AS total_seconds
        \\    FROM runs r
        \\    JOIN usage_ledger ul ON ul.run_id = r.run_id AND ul.source = 'runtime_stage'
        \\    WHERE r.workspace_id = $1
        \\      AND r.state IN ('DONE', 'NOTIFIED_BLOCKED')
        \\    GROUP BY r.run_id
        \\    ORDER BY r.updated_at DESC
        \\    LIMIT 50
        \\)
        \\INSERT INTO workspace_latency_baseline
        \\    (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\SELECT $1,
        \\       COALESCE(percentile_cont(0.50) WITHIN GROUP (ORDER BY total_seconds), 0)::BIGINT,
        \\       COALESCE(percentile_cont(0.95) WITHIN GROUP (ORDER BY total_seconds), 0)::BIGINT,
        \\       COUNT(*)::INTEGER,
        \\       $2
        \\FROM completed_runs
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET p50_seconds = EXCLUDED.p50_seconds,
        \\    p95_seconds = EXCLUDED.p95_seconds,
        \\    sample_count = EXCLUDED.sample_count,
        \\    computed_at = EXCLUDED.computed_at
    , .{ workspace_id, now_ms }) catch |err| {
        obs_log.logWarnErr(.scoring, err, "latency baseline update failed workspace_id={s}", .{workspace_id});
        return;
    };
    q.deinit();
}

pub fn queryScoringConfig(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !types.ScoringConfig {
    var q = conn.query(
        "SELECT enable_agent_scoring, agent_scoring_weights_json FROM workspace_entitlements WHERE workspace_id = $1",
        .{workspace_id},
    ) catch |err| return err;
    defer q.deinit();

    const row = (try q.next()) orelse return .{};
    const enabled = try row.get(bool, 0);
    const raw_weights = try row.get([]const u8, 1);

    return .{
        .enabled = enabled,
        .weights = if (enabled) try math.parseWeightsJson(alloc, raw_weights) else types.DEFAULT_WEIGHTS,
    };
}

fn persistScoreRecord(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
) !bool {
    var existing = try conn.query(
        "SELECT 1 FROM agent_run_scores WHERE run_id = $1",
        .{run_id},
    );
    defer existing.deinit();
    if (try existing.next() != null) return false;

    const score_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(score_id);

    var q = try conn.query(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
    , .{ score_id, run_id, agent_id, workspace_id, score, axis_scores_json, weight_snapshot_json, scored_at });
    q.deinit();
    return true;
}

pub fn persistScore(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    score: u8,
    axis_scores_json: []const u8,
    weight_snapshot_json: []const u8,
    scored_at: i64,
) !void {
    if (agent_id.len == 0) return;
    _ = try persistScoreRecord(conn, alloc, run_id, workspace_id, agent_id, score, axis_scores_json, weight_snapshot_json, scored_at);
}
