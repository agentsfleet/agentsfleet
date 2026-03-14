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
const id_format = @import("../types/id_format.zig");

const log = std.log.scoped(.scoring);
const DEFAULT_WEIGHTS_JSON = "{\"completion\":0.4,\"error_rate\":0.3,\"latency\":0.2,\"resource\":0.1}";
pub const SCORE_FORMULA_VERSION = "m9_v1";

// ── Types ──────────────────────────────────────────────────────────────

pub const Tier = enum {
    unranked,
    bronze,
    silver,
    gold,
    elite,

    pub fn label(self: Tier) []const u8 {
        return switch (self) {
            .unranked => "UNRANKED",
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

const ScoringConfig = struct {
    enabled: bool = false,
    weights: Weights = DEFAULT_WEIGHTS,
};

const WeightsDoc = struct {
    completion: ?f64 = null,
    error_rate: ?f64 = null,
    latency: ?f64 = null,
    resource: ?f64 = null,
};

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

pub fn tierFromScore(score: ?u8) Tier {
    const value = score orelse return .unranked;
    if (value >= 90) return .elite;
    if (value >= 70) return .gold;
    if (value >= 40) return .silver;
    return .bronze;
}

fn hasPriorRuns(baseline: ?LatencyBaseline) bool {
    const bl = baseline orelse return false;
    return bl.sample_count > 0;
}

pub fn tierFromRun(score: u8, baseline: ?LatencyBaseline) Tier {
    if (!hasPriorRuns(baseline)) return .unranked;
    if (score >= 90) return .elite;
    if (score >= 70) return .gold;
    if (score >= 40) return .silver;
    return .bronze;
}

fn validateWeights(weights: Weights) !void {
    if (weights.completion <= 0 or weights.error_rate <= 0 or weights.latency <= 0 or weights.resource <= 0) {
        return error.InvalidScoringWeights;
    }

    const sum = weights.completion + weights.error_rate + weights.latency + weights.resource;
    if (@abs(sum - 1.0) > 0.0001) return error.InvalidScoringWeights;
}

fn parseWeightsJson(alloc: std.mem.Allocator, raw: []const u8) !Weights {
    const parsed = try std.json.parseFromSlice(WeightsDoc, alloc, raw, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();

    const weights = Weights{
        .completion = parsed.value.completion orelse DEFAULT_WEIGHTS.completion,
        .error_rate = parsed.value.error_rate orelse DEFAULT_WEIGHTS.error_rate,
        .latency = parsed.value.latency orelse DEFAULT_WEIGHTS.latency,
        .resource = parsed.value.resource orelse DEFAULT_WEIGHTS.resource,
    };
    try validateWeights(weights);
    return weights;
}

fn axisScoresJson(alloc: std.mem.Allocator, axes: AxisScores) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .completion = axes.completion,
        .error_rate = axes.error_rate,
        .latency = axes.latency,
        .resource = axes.resource,
    }, .{});
}

fn weightsJson(alloc: std.mem.Allocator, weights: Weights) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, .{
        .completion = weights.completion,
        .error_rate = weights.error_rate,
        .latency = weights.latency,
        .resource = weights.resource,
    }, .{});
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

pub fn queryScoringConfig(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
) !ScoringConfig {
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
        .weights = if (enabled) try parseWeightsJson(alloc, raw_weights) else DEFAULT_WEIGHTS,
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

fn persistScore(
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
        log.warn("scoring failed (non-fatal) run_id={s} workspace_id={s} agent_id={s} err={s}", .{ run_id, workspace_id, agent_id, @errorName(err) });
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
    const config = try queryScoringConfig(conn, std.heap.page_allocator, workspace_id);
    if (!config.enabled) return;

    const baseline = queryLatencyBaseline(conn, workspace_id);

    const axes = AxisScores{
        .completion = computeCompletionScore(s.outcome),
        .error_rate = computeErrorRateScore(s.stages_passed, s.stages_total),
        .latency = computeLatencyScore(total_wall_seconds, baseline),
        .resource = computeResourceScore(),
    };

    const score = computeScore(axes, config.weights);
    const tier = tierFromRun(score, baseline);
    const scored_at = std.time.milliTimestamp();
    const axis_scores_json = try axisScoresJson(std.heap.page_allocator, axes);
    defer std.heap.page_allocator.free(axis_scores_json);
    const weight_snapshot_json = try weightsJson(std.heap.page_allocator, config.weights);
    defer std.heap.page_allocator.free(weight_snapshot_json);

    try persistScore(
        conn,
        std.heap.page_allocator,
        run_id,
        workspace_id,
        agent_id,
        score,
        axis_scores_json,
        weight_snapshot_json,
        scored_at,
    );

    log.info("run scored run_id={s} score={d} tier={s}", .{ run_id, score, tier.label() });

    posthog_events.trackAgentRunScored(
        posthog_client,
        posthog_events.distinctIdOrSystem(requested_by),
        run_id,
        workspace_id,
        agent_id,
        score,
        tier.label(),
        SCORE_FORMULA_VERSION,
        axis_scores_json,
        weight_snapshot_json,
        scored_at,
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
    try std.testing.expectEqual(Tier.unranked, tierFromScore(null));
    try std.testing.expectEqual(Tier.bronze, tierFromScore(0));
    try std.testing.expectEqual(Tier.bronze, tierFromScore(39));
    try std.testing.expectEqual(Tier.silver, tierFromScore(40));
    try std.testing.expectEqual(Tier.silver, tierFromScore(69));
    try std.testing.expectEqual(Tier.gold, tierFromScore(70));
    try std.testing.expectEqual(Tier.gold, tierFromScore(89));
    try std.testing.expectEqual(Tier.elite, tierFromScore(90));
    try std.testing.expectEqual(Tier.elite, tierFromScore(100));
}

test "tier is unranked when workspace has no prior runs" {
    try std.testing.expectEqual(Tier.unranked, tierFromRun(95, null));
    try std.testing.expectEqual(Tier.unranked, tierFromRun(95, .{
        .p50_seconds = 12,
        .p95_seconds = 18,
        .sample_count = 0,
    }));
}

test "tier uses score boundaries once prior runs exist" {
    const baseline = LatencyBaseline{
        .p50_seconds = 10,
        .p95_seconds = 20,
        .sample_count = 1,
    };
    try std.testing.expectEqual(Tier.bronze, tierFromRun(39, baseline));
    try std.testing.expectEqual(Tier.silver, tierFromRun(40, baseline));
    try std.testing.expectEqual(Tier.gold, tierFromRun(70, baseline));
    try std.testing.expectEqual(Tier.elite, tierFromRun(90, baseline));
}

test "determinism: same inputs produce same score" {
    const axes = AxisScores{ .completion = 100, .error_rate = 67, .latency = 80, .resource = 50 };
    const score1 = computeScore(axes, DEFAULT_WEIGHTS);
    const score2 = computeScore(axes, DEFAULT_WEIGHTS);
    try std.testing.expectEqual(score1, score2);
}

test "parse weights json accepts valid workspace override" {
    const weights = try parseWeightsJson(std.testing.allocator, "{\"completion\":0.5,\"error_rate\":0.2,\"latency\":0.2,\"resource\":0.1}");
    try std.testing.expect(std.math.approxEqAbs(f64, 0.5, weights.completion, 0.0001));
    try std.testing.expect(std.math.approxEqAbs(f64, 0.2, weights.error_rate, 0.0001));
    try std.testing.expect(std.math.approxEqAbs(f64, 0.2, weights.latency, 0.0001));
    try std.testing.expect(std.math.approxEqAbs(f64, 0.1, weights.resource, 0.0001));
}

test "parse weights json rejects invalid sum" {
    try std.testing.expectError(error.InvalidScoringWeights, parseWeightsJson(std.testing.allocator, "{\"completion\":0.6,\"error_rate\":0.3,\"latency\":0.2,\"resource\":0.1}"));
}

test "axis and weight snapshots stringify to json" {
    const axes_json = try axisScoresJson(std.testing.allocator, .{
        .completion = 100,
        .error_rate = 67,
        .latency = 50,
        .resource = 50,
    });
    defer std.testing.allocator.free(axes_json);

    const weights_json = try weightsJson(std.testing.allocator, DEFAULT_WEIGHTS);
    defer std.testing.allocator.free(weights_json);

    try std.testing.expect(std.mem.containsAtLeast(u8, axes_json, 1, "\"completion\":100"));
    try std.testing.expect(std.mem.containsAtLeast(u8, weights_json, 1, "\"resource\":0.1"));
}

fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    const db = @import("../db/pool.zig");
    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
    const host = opts.connect.host orelse return null;
    const port = opts.connect.port orelse 5432;
    const probe = std.net.tcpConnectToHost(alloc, host, port) catch return null;
    probe.close();
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

fn createTempScoringTables(conn: *pg.Conn) !void {
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_entitlements (
            \\  entitlement_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL UNIQUE,
            \\  plan_tier TEXT NOT NULL,
            \\  max_profiles INTEGER NOT NULL,
            \\  max_stages INTEGER NOT NULL,
            \\  max_distinct_skills INTEGER NOT NULL,
            \\  allow_custom_skills BOOLEAN NOT NULL,
            \\  enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE,
            \\  agent_scoring_weights_json TEXT NOT NULL DEFAULT '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
            \\  created_at BIGINT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE workspace_latency_baseline (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  p50_seconds BIGINT NOT NULL,
            \\  p95_seconds BIGINT NOT NULL,
            \\  sample_count INTEGER NOT NULL,
            \\  computed_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE agent_profiles (
            \\  agent_id TEXT PRIMARY KEY,
            \\  workspace_id TEXT NOT NULL,
            \\  updated_at BIGINT NOT NULL DEFAULT 0
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try conn.query(
            \\CREATE TEMP TABLE agent_run_scores (
            \\  score_id TEXT PRIMARY KEY,
            \\  run_id TEXT NOT NULL UNIQUE,
            \\  agent_id TEXT NOT NULL,
            \\  workspace_id TEXT NOT NULL,
            \\  score INTEGER NOT NULL,
            \\  axis_scores TEXT NOT NULL,
            \\  weight_snapshot TEXT NOT NULL,
            \\  scored_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
}

fn prometheusMetricValue(body: []const u8, name: []const u8) !u64 {
    var lines = std.mem.splitScalar(u8, body, '\n');
    while (lines.next()) |line| {
        if (!std.mem.startsWith(u8, line, name)) continue;
        if (line.len <= name.len + 1) break;
        return std.fmt.parseInt(u64, line[name.len + 1 ..], 10);
    }
    return error.MetricNotFound;
}

test "scoreRunIfTerminal fail-safe catches invalid workspace scoring config" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempScoringTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_1', 'ws_1', 'FREE', 1, 3, 3, false, true, '{"completion":0.6}', 0, 0)
        , .{});
        q.deinit();
    }

    const before_body = try metrics.renderPrometheus(std.testing.allocator, false, 0, 0);
    defer std.testing.allocator.free(before_body);
    const before_failed = try prometheusMetricValue(before_body, "zombie_agent_scoring_failed_total");

    const state = ScoringState{
        .outcome = .done,
        .stages_passed = 1,
        .stages_total = 1,
    };
    scoreRunIfTerminal(db_ctx.conn, null, "run_1", "ws_1", "agent_1", "user_1", &state, 12);

    const after_body = try metrics.renderPrometheus(std.testing.allocator, false, 0, 0);
    defer std.testing.allocator.free(after_body);
    const after_failed = try prometheusMetricValue(after_body, "zombie_agent_scoring_failed_total");

    try std.testing.expectEqual(before_failed + 1, after_failed);
}

test "db-backed scoring config changes final score" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempScoringTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_2', 'ws_2', 'FREE', 1, 3, 3, false, true, '{"completion":0.1,"error_rate":0.7,"latency":0.1,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_latency_baseline
            \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
            \\VALUES ('ws_2', 10, 30, 5, 0)
        , .{});
        q.deinit();
    }

    const config = try queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_2");
    const axes = AxisScores{
        .completion = computeCompletionScore(.done),
        .error_rate = computeErrorRateScore(1, 2),
        .latency = computeLatencyScore(30, .{
            .p50_seconds = 10,
            .p95_seconds = 30,
            .sample_count = 5,
        }),
        .resource = computeResourceScore(),
    };

    const default_score = computeScore(axes, DEFAULT_WEIGHTS);
    const configured_score = computeScore(axes, config.weights);

    try std.testing.expect(config.enabled);
    try std.testing.expect(default_score != configured_score);
    try std.testing.expectEqual(@as(u8, 50), configured_score);
}

test "scoreRunIfTerminal persists run score" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempScoringTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_3', 'ws_3', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_latency_baseline
            \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
            \\VALUES ('ws_3', 10, 30, 5, 0)
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO agent_profiles (agent_id, workspace_id, updated_at)
            \\VALUES ('agent_3', 'ws_3', 0)
        , .{});
        q.deinit();
    }

    const state = ScoringState{
        .outcome = .done,
        .stages_passed = 2,
        .stages_total = 2,
    };
    scoreRunIfTerminal(db_ctx.conn, null, "run_3", "ws_3", "agent_3", "user_3", &state, 8);

    {
        var q = try db_ctx.conn.query(
            "SELECT score, agent_id FROM agent_run_scores WHERE run_id = 'run_3'",
            .{},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i32, 95), row.get(i32, 0) catch -1);
        try std.testing.expectEqualStrings("agent_3", row.get([]const u8, 1) catch "");
    }
}
