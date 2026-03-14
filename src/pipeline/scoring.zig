//! Agent run quality scoring engine (M9_001).
//!
//! Computes a deterministic 0–100 score from four weighted axes after
//! every run reaches a terminal state.  The scoring call is fail-safe:
//! errors are logged and metrics incremented, but the run is never
//! blocked or failed by scoring.

const std = @import("std");
const pg = @import("pg");
const posthog = @import("posthog");
const posthog_events = @import("../observability/posthog_events.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");

const log = std.log.scoped(.scoring);

// ── Types ──────────────────────────────────────────────────────────────

pub const Tier = enum {
    bronze,
    silver,
    gold,
    elite,

    pub fn label(self: Tier) []const u8 {
        return switch (self) {
            .bronze => "BRONZE",
            .silver => "SILVER",
            .gold => "GOLD",
            .elite => "ELITE",
        };
    }
};

pub const TerminalOutcome = enum {
    pending,
    done,
    blocked_stage_graph,
    blocked_retries_exhausted,
    error_propagation,
};

pub const AxisScores = struct {
    completion: u8 = 0,
    error_rate: u8 = 0,
    latency: u8 = 0,
    resource: u8 = 0,
};

pub const Weights = struct {
    completion: f64 = 0.4,
    error_rate: f64 = 0.3,
    latency: f64 = 0.2,
    resource: f64 = 0.1,
};

pub const DEFAULT_WEIGHTS = Weights{};

pub const LatencyBaseline = struct {
    p50_seconds: u64,
    p95_seconds: u64,
    sample_count: u32,
};

/// Mutable state accumulated during a run.  Declared in `executeRun`,
/// mutated as stages complete, and read by the scoring defer.
pub const ScoringState = struct {
    outcome: TerminalOutcome = .pending,
    stages_passed: u32 = 0,
    stages_total: u32 = 0,
};

// ── Pure scoring functions ─────────────────────────────────────────────

pub fn computeCompletionScore(outcome: TerminalOutcome) u8 {
    return switch (outcome) {
        .done => 100,
        .blocked_retries_exhausted => 30,
        .blocked_stage_graph => 10,
        .error_propagation => 0,
        .pending => 0,
    };
}

pub fn computeErrorRateScore(passed: u32, total: u32) u8 {
    if (total == 0) return 0;
    // Rounded: (passed * 100 + total/2) / total
    const numerator = @as(u64, passed) * 100 + @as(u64, total) / 2;
    const result = @divFloor(numerator, @as(u64, total));
    return @intCast(@min(result, 100));
}

pub fn computeLatencyScore(wall_seconds: u64, baseline: ?LatencyBaseline) u8 {
    const bl = baseline orelse return 50; // neutral if no baseline
    if (bl.sample_count < 5) return 50; // neutral if < 5 prior runs
    if (bl.p50_seconds == 0) {
        return if (wall_seconds == 0) 100 else 0;
    }
    if (wall_seconds <= bl.p50_seconds) return 100;

    // Linear degradation: 100 at p50 → 0 at 3×p50
    const range = bl.p50_seconds * 2; // 3×p50 − p50
    const excess = wall_seconds - bl.p50_seconds;
    if (excess >= range) return 0;

    return @intCast(100 - @divFloor(excess * 100, range));
}

/// STUBBED at 50 until M4_008 (Firecracker sandbox) provides metrics.
pub fn computeResourceScore() u8 {
    return 50;
}

pub fn computeScore(axes: AxisScores, weights: Weights) u8 {
    const raw: f64 =
        @as(f64, @floatFromInt(axes.completion)) * weights.completion +
        @as(f64, @floatFromInt(axes.error_rate)) * weights.error_rate +
        @as(f64, @floatFromInt(axes.latency)) * weights.latency +
        @as(f64, @floatFromInt(axes.resource)) * weights.resource;

    const rounded = @as(i64, @intFromFloat(@round(raw)));
    return @intCast(@as(u64, @intCast(std.math.clamp(rounded, 0, 100))));
}

pub fn tierFromScore(score: u8) Tier {
    if (score >= 90) return .elite;
    if (score >= 70) return .gold;
    if (score >= 40) return .silver;
    return .bronze;
}

// ── DB functions ───────────────────────────────────────────────────────

pub fn queryLatencyBaseline(conn: *pg.Conn, workspace_id: []const u8) ?LatencyBaseline {
    var q = conn.query(
        \\SELECT p50_seconds, p95_seconds, sample_count
        \\FROM workspace_latency_baseline
        \\WHERE workspace_id = $1
    , .{workspace_id}) catch return null;
    defer q.deinit();

    const row = (q.next() catch return null) orelse return null;
    return LatencyBaseline{
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

pub fn isScoringEnabled(conn: *pg.Conn, workspace_id: []const u8) bool {
    var q = conn.query(
        "SELECT enable_agent_scoring FROM workspace_entitlements WHERE workspace_id = $1",
        .{workspace_id},
    ) catch return false;
    defer q.deinit();

    const row = (q.next() catch return false) orelse return false;
    return row.get(bool, 0) catch false;
}

// ── Orchestrator ───────────────────────────────────────────────────────

/// Fail-safe entry point called from a `defer` block in `executeRun`.
/// Catches all errors internally — scoring must NEVER block or fail a run.
pub fn scoreRunIfTerminal(
    conn: *pg.Conn,
    posthog_client: ?*posthog.PostHogClient,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    requested_by: []const u8,
    state: *const ScoringState,
    total_wall_seconds: u64,
) void {
    if (state.outcome == .pending) return;

    const start_ns = std.time.nanoTimestamp();
    scoreRunInner(
        conn,
        posthog_client,
        run_id,
        workspace_id,
        agent_id,
        requested_by,
        state,
        total_wall_seconds,
    ) catch |err| {
        log.warn("scoring failed (non-fatal) run_id={s} err={s}", .{ run_id, @errorName(err) });
        metrics.incAgentScoringFailed();
        posthog_events.trackAgentScoringFailed(
            posthog_client,
            posthog_events.distinctIdOrSystem(requested_by),
            run_id,
            workspace_id,
            @errorName(err),
        );
    };
    const elapsed_ns = std.time.nanoTimestamp() - start_ns;
    const elapsed_ms: u64 = @intCast(@max(@divFloor(elapsed_ns, 1_000_000), 0));
    metrics.observeAgentScoringDurationMs(elapsed_ms);
}

fn scoreRunInner(
    conn: *pg.Conn,
    posthog_client: ?*posthog.PostHogClient,
    run_id: []const u8,
    workspace_id: []const u8,
    agent_id: []const u8,
    requested_by: []const u8,
    s: *const ScoringState,
    total_wall_seconds: u64,
) !void {
    if (!isScoringEnabled(conn, workspace_id)) return;

    const baseline = queryLatencyBaseline(conn, workspace_id);

    const axes = AxisScores{
        .completion = computeCompletionScore(s.outcome),
        .error_rate = computeErrorRateScore(s.stages_passed, s.stages_total),
        .latency = computeLatencyScore(total_wall_seconds, baseline),
        .resource = computeResourceScore(),
    };

    const score = computeScore(axes, DEFAULT_WEIGHTS);
    const tier = tierFromScore(score);

    log.info("run scored run_id={s} score={d} tier={s}", .{ run_id, score, tier.label() });

    posthog_events.trackAgentRunScored(
        posthog_client,
        posthog_events.distinctIdOrSystem(requested_by),
        run_id,
        workspace_id,
        agent_id,
        score,
        tier.label(),
        axes.completion,
        axes.error_rate,
        axes.latency,
        axes.resource,
    );

    metrics.incAgentScoreComputed(tier);
    metrics.setAgentScoreLatest(score);

    updateLatencyBaseline(conn, workspace_id);
}

// ── Tests ──────────────────────────────────────────────────────────────

test "completion score maps outcomes correctly" {
    try std.testing.expectEqual(@as(u8, 100), computeCompletionScore(.done));
    try std.testing.expectEqual(@as(u8, 30), computeCompletionScore(.blocked_retries_exhausted));
    try std.testing.expectEqual(@as(u8, 10), computeCompletionScore(.blocked_stage_graph));
    try std.testing.expectEqual(@as(u8, 0), computeCompletionScore(.error_propagation));
    try std.testing.expectEqual(@as(u8, 0), computeCompletionScore(.pending));
}

test "error rate score: boundary values" {
    try std.testing.expectEqual(@as(u8, 0), computeErrorRateScore(0, 0));
    try std.testing.expectEqual(@as(u8, 100), computeErrorRateScore(3, 3));
    try std.testing.expectEqual(@as(u8, 67), computeErrorRateScore(2, 3));
    try std.testing.expectEqual(@as(u8, 33), computeErrorRateScore(1, 3));
    try std.testing.expectEqual(@as(u8, 0), computeErrorRateScore(0, 3));
    try std.testing.expectEqual(@as(u8, 50), computeErrorRateScore(1, 2));
    try std.testing.expectEqual(@as(u8, 100), computeErrorRateScore(1, 1));
}

test "latency score: neutral when no baseline" {
    try std.testing.expectEqual(@as(u8, 50), computeLatencyScore(10, null));
}

test "latency score: neutral when < 5 prior runs" {
    try std.testing.expectEqual(@as(u8, 50), computeLatencyScore(10, .{ .p50_seconds = 5, .p95_seconds = 10, .sample_count = 4 }));
}

test "latency score: at or below p50 returns 100" {
    const bl = LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 100), computeLatencyScore(5, bl));
    try std.testing.expectEqual(@as(u8, 100), computeLatencyScore(10, bl));
}

test "latency score: at 3x p50 returns 0" {
    const bl = LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 0), computeLatencyScore(30, bl));
    try std.testing.expectEqual(@as(u8, 0), computeLatencyScore(50, bl));
}

test "latency score: linear degradation between p50 and 3x p50" {
    const bl = LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    // At 2x p50 (excess=10, range=20): 100 - 10*100/20 = 50
    try std.testing.expectEqual(@as(u8, 50), computeLatencyScore(20, bl));
}

test "latency score: p50=0 edge case" {
    const bl = LatencyBaseline{ .p50_seconds = 0, .p95_seconds = 0, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 100), computeLatencyScore(0, bl));
    try std.testing.expectEqual(@as(u8, 0), computeLatencyScore(1, bl));
}

test "resource score is stubbed at 50" {
    try std.testing.expectEqual(@as(u8, 50), computeResourceScore());
}

test "computeScore: deterministic with default weights" {
    const axes = AxisScores{ .completion = 100, .error_rate = 100, .latency = 100, .resource = 50 };
    // 100*0.4 + 100*0.3 + 100*0.2 + 50*0.1 = 40 + 30 + 20 + 5 = 95
    try std.testing.expectEqual(@as(u8, 95), computeScore(axes, DEFAULT_WEIGHTS));
}

test "computeScore: all zeros" {
    const axes = AxisScores{ .completion = 0, .error_rate = 0, .latency = 0, .resource = 0 };
    try std.testing.expectEqual(@as(u8, 0), computeScore(axes, DEFAULT_WEIGHTS));
}

test "computeScore: perfect run with stub resource" {
    // DONE, all stages pass, at p50 baseline, resource stubbed
    const axes = AxisScores{ .completion = 100, .error_rate = 100, .latency = 100, .resource = 50 };
    try std.testing.expectEqual(@as(u8, 95), computeScore(axes, DEFAULT_WEIGHTS));
}

test "computeScore: failed run" {
    const axes = AxisScores{ .completion = 0, .error_rate = 0, .latency = 50, .resource = 50 };
    // 0*0.4 + 0*0.3 + 50*0.2 + 50*0.1 = 0 + 0 + 10 + 5 = 15
    try std.testing.expectEqual(@as(u8, 15), computeScore(axes, DEFAULT_WEIGHTS));
}

test "tier boundaries" {
    try std.testing.expectEqual(Tier.bronze, tierFromScore(0));
    try std.testing.expectEqual(Tier.bronze, tierFromScore(39));
    try std.testing.expectEqual(Tier.silver, tierFromScore(40));
    try std.testing.expectEqual(Tier.silver, tierFromScore(69));
    try std.testing.expectEqual(Tier.gold, tierFromScore(70));
    try std.testing.expectEqual(Tier.gold, tierFromScore(89));
    try std.testing.expectEqual(Tier.elite, tierFromScore(90));
    try std.testing.expectEqual(Tier.elite, tierFromScore(100));
}

test "determinism: same inputs produce same score" {
    const axes = AxisScores{ .completion = 100, .error_rate = 67, .latency = 80, .resource = 50 };
    const score1 = computeScore(axes, DEFAULT_WEIGHTS);
    const score2 = computeScore(axes, DEFAULT_WEIGHTS);
    try std.testing.expectEqual(score1, score2);
}
