const std = @import("std");
const pg = @import("pg");
const metrics = @import("../observability/metrics.zig");
const scoring = @import("scoring.zig");
const common = @import("../http/handlers/common.zig");

fn createTempScoringTables(conn: *pg.Conn) !void {
    // Rule 1: exec() for DDL (internal drain loop, always leaves _state=.idle)
    // Rule 5: no ON COMMIT DROP (exec() auto-commits; table would drop immediately)
    _ = try conn.exec(
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
        \\  enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE,
        \\  scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE workspace_latency_baseline (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  p50_seconds BIGINT NOT NULL,
        \\  p95_seconds BIGINT NOT NULL,
        \\  sample_count INTEGER NOT NULL,
        \\  computed_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE agent_profiles (
        \\  agent_id TEXT PRIMARY KEY,
        \\  workspace_id TEXT NOT NULL,
        \\  trust_streak_runs INTEGER NOT NULL DEFAULT 0,
        \\  trust_level TEXT NOT NULL DEFAULT 'UNEARNED',
        \\  last_scored_at BIGINT,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE agent_run_analysis (
        \\  analysis_id TEXT PRIMARY KEY,
        \\  run_id TEXT NOT NULL UNIQUE,
        \\  agent_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  failure_class TEXT,
        \\  failure_is_infra BOOLEAN NOT NULL DEFAULT FALSE,
        \\  failure_signals JSONB NOT NULL DEFAULT '[]'::jsonb,
        \\  improvement_hints JSONB NOT NULL DEFAULT '[]'::jsonb,
        \\  stderr_tail TEXT,
        \\  analyzed_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE agent_run_scores (
        \\  score_id TEXT PRIMARY KEY,
        \\  run_id TEXT NOT NULL UNIQUE,
        \\  agent_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  score INTEGER NOT NULL,
        \\  axis_scores TEXT NOT NULL,
        \\  weight_snapshot TEXT NOT NULL,
        \\  scored_at BIGINT NOT NULL
        \\)
    , .{});
}

fn insertScoringWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    var entitlement_id_buf: [96]u8 = undefined;
    const entitlement_id = try std.fmt.bufPrint(&entitlement_id_buf, "ent_{s}", .{workspace_id});
    // Rule 1: exec() for INSERT — has internal drain loop, always leaves _state=.idle
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{ entitlement_id, workspace_id });
    _ = try conn.exec(
        \\INSERT INTO workspace_latency_baseline
        \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\VALUES ($1, 10, 30, 5, 0)
    , .{workspace_id});
}

fn insertAgentProfile(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    trust_streak_runs: i32,
    trust_level: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_profiles
        \\  (agent_id, workspace_id, trust_streak_runs, trust_level, last_scored_at, updated_at)
        \\VALUES ($1, $2, $3, $4, 0, 0)
    , .{ agent_id, workspace_id, trust_streak_runs, trust_level });
}

fn insertHistoricalScore(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    run_id: []const u8,
    score: i32,
    scored_at: i64,
    failure_class: ?[]const u8,
    failure_is_infra: bool,
) !void {
    var analysis_id_buf: [128]u8 = undefined;
    _ = try conn.exec(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, '{}', '{}', $6)
    , .{ run_id, run_id, agent_id, workspace_id, score, scored_at });

    if (failure_class) |class| {
        const analysis_id = try std.fmt.bufPrint(&analysis_id_buf, "analysis_{s}", .{run_id});
        _ = try conn.exec(
            \\INSERT INTO agent_run_analysis
            \\  (analysis_id, run_id, agent_id, workspace_id, failure_class, failure_is_infra, failure_signals, improvement_hints, stderr_tail, analyzed_at)
            \\VALUES ($1, $2, $3, $4, $5, $6, '[]'::jsonb, '[]'::jsonb, NULL, $7)
        , .{ analysis_id, run_id, agent_id, workspace_id, class, failure_is_infra, scored_at });
    }
}

fn expectTrustState(conn: *pg.Conn, agent_id: []const u8, expected_streak: i32, expected_level: []const u8) !void {
    var q = try conn.query(
        "SELECT trust_streak_runs, trust_level FROM agent_profiles WHERE agent_id = $1",
        .{agent_id},
    );
    const row = (try q.next()) orelse { q.deinit(); return error.TestUnexpectedResult; };
    try std.testing.expectEqual(expected_streak, row.get(i32, 0) catch -1);
    try std.testing.expectEqualStrings(expected_level, row.get([]const u8, 1) catch "");
    _ = q.next() catch {}; // Rule 2: drain 'C'+'Z' → _state=.idle before pool.release()
    q.deinit();
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

test "completion score maps outcomes correctly" {
    try std.testing.expectEqual(@as(u8, 100), scoring.computeCompletionScore(.done));
    try std.testing.expectEqual(@as(u8, 30), scoring.computeCompletionScore(.blocked_retries_exhausted));
    try std.testing.expectEqual(@as(u8, 10), scoring.computeCompletionScore(.blocked_stage_graph));
    try std.testing.expectEqual(@as(u8, 0), scoring.computeCompletionScore(.error_propagation));
    try std.testing.expectEqual(@as(u8, 0), scoring.computeCompletionScore(.pending));
}

test "error rate score: boundary values" {
    try std.testing.expectEqual(@as(u8, 0), scoring.computeErrorRateScore(0, 0));
    try std.testing.expectEqual(@as(u8, 100), scoring.computeErrorRateScore(3, 3));
    try std.testing.expectEqual(@as(u8, 67), scoring.computeErrorRateScore(2, 3));
    try std.testing.expectEqual(@as(u8, 33), scoring.computeErrorRateScore(1, 3));
    try std.testing.expectEqual(@as(u8, 0), scoring.computeErrorRateScore(0, 3));
    try std.testing.expectEqual(@as(u8, 50), scoring.computeErrorRateScore(1, 2));
    try std.testing.expectEqual(@as(u8, 100), scoring.computeErrorRateScore(1, 1));
}

test "latency score: neutral when no baseline" {
    try std.testing.expectEqual(@as(u8, 50), scoring.computeLatencyScore(10, null));
}

test "latency score: neutral when < 5 prior runs" {
    try std.testing.expectEqual(@as(u8, 50), scoring.computeLatencyScore(10, .{ .p50_seconds = 5, .p95_seconds = 10, .sample_count = 4 }));
}

test "latency score: at or below p50 returns 100" {
    const bl = scoring.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 100), scoring.computeLatencyScore(5, bl));
    try std.testing.expectEqual(@as(u8, 100), scoring.computeLatencyScore(10, bl));
}

test "latency score: at 3x p50 returns 0" {
    const bl = scoring.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 0), scoring.computeLatencyScore(30, bl));
    try std.testing.expectEqual(@as(u8, 0), scoring.computeLatencyScore(50, bl));
}

test "latency score: linear degradation between p50 and 3x p50" {
    const bl = scoring.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 50), scoring.computeLatencyScore(20, bl));
}

test "latency score: p50=0 edge case" {
    const bl = scoring.LatencyBaseline{ .p50_seconds = 0, .p95_seconds = 0, .sample_count = 10 };
    try std.testing.expectEqual(@as(u8, 100), scoring.computeLatencyScore(0, bl));
    try std.testing.expectEqual(@as(u8, 0), scoring.computeLatencyScore(1, bl));
}

test "resource score is stubbed at 50" {
    try std.testing.expectEqual(@as(u8, 50), scoring.computeResourceScore());
}

test "computeScore: deterministic with default weights" {
    const axes = scoring.AxisScores{ .completion = 100, .error_rate = 100, .latency = 100, .resource = 50 };
    try std.testing.expectEqual(@as(u8, 95), scoring.computeScore(axes, scoring.DEFAULT_WEIGHTS));
}

test "computeScore: all zeros" {
    const axes = scoring.AxisScores{ .completion = 0, .error_rate = 0, .latency = 0, .resource = 0 };
    try std.testing.expectEqual(@as(u8, 0), scoring.computeScore(axes, scoring.DEFAULT_WEIGHTS));
}

test "computeScore: failed run" {
    const axes = scoring.AxisScores{ .completion = 0, .error_rate = 0, .latency = 50, .resource = 50 };
    try std.testing.expectEqual(@as(u8, 15), scoring.computeScore(axes, scoring.DEFAULT_WEIGHTS));
}

test "tier boundaries" {
    try std.testing.expectEqual(scoring.Tier.unranked, scoring.tierFromScore(null));
    try std.testing.expectEqual(scoring.Tier.bronze, scoring.tierFromScore(0));
    try std.testing.expectEqual(scoring.Tier.bronze, scoring.tierFromScore(39));
    try std.testing.expectEqual(scoring.Tier.silver, scoring.tierFromScore(40));
    try std.testing.expectEqual(scoring.Tier.silver, scoring.tierFromScore(69));
    try std.testing.expectEqual(scoring.Tier.gold, scoring.tierFromScore(70));
    try std.testing.expectEqual(scoring.Tier.gold, scoring.tierFromScore(89));
    try std.testing.expectEqual(scoring.Tier.elite, scoring.tierFromScore(90));
    try std.testing.expectEqual(scoring.Tier.elite, scoring.tierFromScore(100));
}

test "tier uses score boundaries once prior runs exist" {
    const baseline = scoring.LatencyBaseline{ .p50_seconds = 10, .p95_seconds = 20, .sample_count = 1 };
    try std.testing.expectEqual(scoring.Tier.bronze, scoring.tierFromRun(39, baseline));
    try std.testing.expectEqual(scoring.Tier.silver, scoring.tierFromRun(40, baseline));
    try std.testing.expectEqual(scoring.Tier.gold, scoring.tierFromRun(70, baseline));
    try std.testing.expectEqual(scoring.Tier.elite, scoring.tierFromRun(90, baseline));
}

test "parse weights json accepts valid workspace override" {
    const weights = try scoring.parseWeightsJson(std.testing.allocator, "{\"completion\":0.5,\"error_rate\":0.2,\"latency\":0.2,\"resource\":0.1}");
    try std.testing.expect(std.math.approxEqAbs(f64, 0.5, weights.completion, 0.0001));
    try std.testing.expect(std.math.approxEqAbs(f64, 0.2, weights.error_rate, 0.0001));
}

test "parse weights json rejects invalid sum" {
    try std.testing.expectError(error.InvalidScoringWeights, scoring.parseWeightsJson(std.testing.allocator, "{\"completion\":0.6,\"error_rate\":0.3,\"latency\":0.2,\"resource\":0.1}"));
}

test "scoreRunIfTerminal fail-safe catches invalid workspace scoring config" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_1', 'ws_1', 'FREE', 1, 3, 3, false, true, '{"completion":0.6}', 0, 0)
    , .{});

    const before_body = try metrics.renderPrometheus(std.testing.allocator, false, 0, 0);
    defer std.testing.allocator.free(before_body);
    const before_failed = try prometheusMetricValue(before_body, "zombie_agent_scoring_failed_total");

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 1, .stages_total = 1 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_1", "ws_1", "agent_1", "user_1", &state, 12);

    const after_body = try metrics.renderPrometheus(std.testing.allocator, false, 0, 0);
    defer std.testing.allocator.free(after_body);
    const after_failed = try prometheusMetricValue(after_body, "zombie_agent_scoring_failed_total");

    try std.testing.expectEqual(before_failed + 1, after_failed);
}

test "scoreRunIfTerminal persists run score" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_3', 'ws_3', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_latency_baseline
        \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\VALUES ('ws_3', 10, 30, 5, 0)
    , .{});

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_3", "ws_3", "agent_3", "user_3", &state, 8);

    {
        var q = try db_ctx.conn.query("SELECT score, agent_id FROM agent_run_scores WHERE run_id = 'run_3'", .{});
        const row = (try q.next()) orelse { q.deinit(); return error.TestUnexpectedResult; };
        try std.testing.expectEqual(@as(i32, 95), row.get(i32, 0) catch -1);
        try std.testing.expectEqualStrings("agent_3", row.get([]const u8, 1) catch "");
        _ = q.next() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query("SELECT failure_class, failure_is_infra FROM agent_run_analysis WHERE run_id = 'run_3'", .{});
        const row = (try q.next()) orelse { q.deinit(); return error.TestUnexpectedResult; };
        try std.testing.expect((row.get(?[]const u8, 0) catch null) == null);
        try std.testing.expectEqual(false, row.get(bool, 1) catch true);
        _ = q.next() catch {}; // drain
        q.deinit();
    }
}

test "scoreRunIfTerminal gold run increments streak and earns trusted status" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    try insertScoringWorkspace(db_ctx.conn, "ws_trust_earned");
    try insertAgentProfile(db_ctx.conn, "agent_trust_earned", "ws_trust_earned", 9, "UNEARNED");

    var idx: usize = 0;
    while (idx < 9) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "hist_gold_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try insertHistoricalScore(db_ctx.conn, "agent_trust_earned", "ws_trust_earned", run_id, 95, @intCast(idx + 1), null, false);
    }

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_trust_earned", "ws_trust_earned", "agent_trust_earned", "user_earned", &state, 8);

    try expectTrustState(db_ctx.conn, "agent_trust_earned", 10, "TRUSTED");
}

test "scoreRunIfTerminal infra failure does not reset trusted streak" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    try insertScoringWorkspace(db_ctx.conn, "ws_trust_infra");
    try insertAgentProfile(db_ctx.conn, "agent_trust_infra", "ws_trust_infra", 10, "TRUSTED");

    var idx: usize = 0;
    while (idx < 10) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "hist_infra_gold_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try insertHistoricalScore(db_ctx.conn, "agent_trust_infra", "ws_trust_infra", run_id, 95, @intCast(idx + 1), null, false);
    }

    const state = scoring.ScoringState{ .outcome = .blocked_retries_exhausted, .stages_passed = 1, .stages_total = 1 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_trust_infra", "ws_trust_infra", "agent_trust_infra", "user_infra", &state, 8);

    try expectTrustState(db_ctx.conn, "agent_trust_infra", 10, "TRUSTED");
}

test "scoreRunIfTerminal agent-attributable low score resets trusted streak" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    try insertScoringWorkspace(db_ctx.conn, "ws_trust_reset");
    try insertAgentProfile(db_ctx.conn, "agent_trust_reset", "ws_trust_reset", 10, "TRUSTED");

    var idx: usize = 0;
    while (idx < 10) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "hist_reset_gold_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try insertHistoricalScore(db_ctx.conn, "agent_trust_reset", "ws_trust_reset", run_id, 95, @intCast(idx + 1), null, false);
    }

    const state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 1 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_trust_reset", "ws_trust_reset", "agent_trust_reset", "user_reset", &state, 8);

    try expectTrustState(db_ctx.conn, "agent_trust_reset", 0, "UNEARNED");
}

test "buildScoringContextForEcho returns orientation when no history" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('ent_ctx_1', 'ws_ctx_1', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', true, 2048, 0, 0)
    , .{});

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_ctx_1");
    const block = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_ctx_1", "agent_ctx_1", cfg);
    defer std.testing.allocator.free(block);

    try std.testing.expect(std.mem.containsAtLeast(u8, block, 1, "You have no prior score history"));
}

test "buildScoringContextForEcho is empty when injection disabled" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('ent_ctx_2', 'ws_ctx_2', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', false, 2048, 0, 0)
    , .{});

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_ctx_2");
    const block = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_ctx_2", "agent_ctx_2", cfg);
    defer std.testing.allocator.free(block);

    try std.testing.expectEqual(@as(usize, 0), block.len);
}
