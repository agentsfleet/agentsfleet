const std = @import("std");
const pg = @import("pg");
const metrics = @import("../observability/metrics.zig");
const agents = @import("agents.zig");
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
        \\  status TEXT NOT NULL DEFAULT 'ACTIVE',
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
        \\  proposal_id TEXT,
        \\  score INTEGER NOT NULL,
        \\  axis_scores TEXT NOT NULL,
        \\  weight_snapshot TEXT NOT NULL,
        \\  scored_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE agent_improvement_proposals (
        \\  proposal_id TEXT PRIMARY KEY,
        \\  agent_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  trigger_reason TEXT NOT NULL,
        \\  proposed_changes TEXT NOT NULL,
        \\  config_version_id TEXT NOT NULL,
        \\  approval_mode TEXT NOT NULL,
        \\  generation_status TEXT NOT NULL,
        \\  status TEXT NOT NULL,
        \\  rejection_reason TEXT,
        \\  auto_apply_at BIGINT,
        \\  applied_by TEXT,
        \\  created_at BIGINT NOT NULL,
        \\  updated_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE harness_change_log (
        \\  change_id TEXT PRIMARY KEY,
        \\  agent_id TEXT NOT NULL,
        \\  proposal_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL,
        \\  field_name TEXT NOT NULL,
        \\  old_value TEXT NOT NULL,
        \\  new_value TEXT NOT NULL,
        \\  applied_at BIGINT NOT NULL,
        \\  applied_by TEXT NOT NULL,
        \\  reverted_from TEXT,
        \\  score_delta DOUBLE PRECISION
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  agent_id TEXT NOT NULL,
        \\  version INTEGER NOT NULL DEFAULT 1,
        \\  source_markdown TEXT NOT NULL DEFAULT '{}',
        \\  compiled_profile_json TEXT,
        \\  compile_engine TEXT NOT NULL DEFAULT 'deterministic-v1',
        \\  validation_report_json TEXT NOT NULL DEFAULT '{}',
        \\  is_valid BOOLEAN NOT NULL DEFAULT FALSE,
        \\  created_at BIGINT NOT NULL DEFAULT 0,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  config_version_id TEXT NOT NULL,
        \\  activated_by TEXT NOT NULL DEFAULT 'test',
        \\  activated_at BIGINT NOT NULL DEFAULT 0
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
    const row = (try q.next()) orelse {
        q.deinit();
        return error.TestUnexpectedResult;
    };
    try std.testing.expectEqual(expected_streak, row.get(i32, 0) catch -1);
    try std.testing.expectEqualStrings(expected_level, row.get([]const u8, 1) catch "");
    q.drain() catch {}; // Rule 2: drain 'C'+'Z' → _state=.idle before pool.release()
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

fn demoTimeoutReductionSkill(
    alloc: std.mem.Allocator,
    role_id: []const u8,
    skill_id: []const u8,
    input: agents.RoleInput,
) anyerror!agents.AgentResult {
    _ = alloc;
    _ = role_id;
    _ = skill_id;
    const spec_content = input.spec_content orelse return error.MissingRoleInput;
    if (!std.mem.startsWith(u8, spec_content, "run-")) return error.InvalidRunId;
    const run_number = try std.fmt.parseUnsigned(u32, spec_content["run-".len..], 10);
    const memory_context = input.memory_context orelse "";
    const has_timeout_guidance = std.mem.containsAtLeast(u8, memory_context, 1, "TIMEOUT");
    const should_timeout = if (has_timeout_guidance)
        run_number == 1 or run_number == 6
    else
        run_number <= 6;

    if (should_timeout) return error.CommandTimedOut;
    return .{
        .content = "verdict: PASS",
        .token_count = 32,
        .wall_seconds = 1,
        .exit_ok = true,
    };
}

fn runTimeoutDemoCohort(memory_context: []const u8) !u32 {
    // check-pg-drain: ok — no conn.query() here; checker misattributes test-block queries
    var registry = agents.SkillRegistry.init(std.testing.allocator);
    defer registry.deinit();
    try registry.registerCustomSkill("timeout-demo", .echo, demoTimeoutReductionSkill);

    const binding = agents.resolveRoleWithRegistry(&registry, "planner", "timeout-demo") orelse return error.TestExpectedRole;
    const prompts = agents.PromptFiles{
        .echo = "echo prompt",
        .scout = "scout prompt",
        .warden = "warden prompt",
    };

    var timeout_count: u32 = 0;
    var run_number: u32 = 1;
    while (run_number <= 10) : (run_number += 1) {
        const spec_content = try std.fmt.allocPrint(std.testing.allocator, "run-{d}", .{run_number});
        defer std.testing.allocator.free(spec_content);

        _ = agents.runByRole(std.testing.allocator, binding, .{
            .workspace_path = "/tmp",
            .prompts = &prompts,
            .spec_content = spec_content,
            .memory_context = memory_context,
        }) catch |err| switch (err) {
            error.CommandTimedOut => {
                timeout_count += 1;
                continue;
            },
            else => return err,
        };
    }

    return timeout_count;
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
        const row = (try q.next()) orelse {
            q.deinit();
            return error.TestUnexpectedResult;
        };
        try std.testing.expectEqual(@as(i32, 95), row.get(i32, 0) catch -1);
        try std.testing.expectEqualStrings("agent_3", row.get([]const u8, 1) catch "");
        q.drain() catch {}; // drain CommandComplete + ReadyForQuery → state = .idle
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query("SELECT failure_class, failure_is_infra FROM agent_run_analysis WHERE run_id = 'run_3'", .{});
        const row = (try q.next()) orelse {
            q.deinit();
            return error.TestUnexpectedResult;
        };
        try std.testing.expect((row.get(?[]const u8, 0) catch null) == null);
        try std.testing.expectEqual(false, row.get(bool, 1) catch true);
        q.drain() catch {}; // drain
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

test "scoreRunIfTerminal tags post-change window and computes score delta" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    try insertScoringWorkspace(db_ctx.conn, "ws_improve");
    try insertAgentProfile(db_ctx.conn, "agent_improve", "ws_improve", 0, "UNEARNED");

    var hist_idx: usize = 0;
    while (hist_idx < 5) : (hist_idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "hist_improve_{d}", .{hist_idx});
        defer std.testing.allocator.free(run_id);
        try insertHistoricalScore(db_ctx.conn, "agent_improve", "ws_improve", run_id, 40, @intCast(hist_idx + 1), null, false);
    }

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6aa1', 'agent_improve', 'ws_improve', 'DECLINING_SCORE', '[]', 'cfg_1', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 100, 100)
    , .{});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6aa2', 'agent_improve', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6aa1', 'ws_improve', 'stage_insert', '{}', '{}', 100, 'operator:test', NULL)
    , .{});

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    var run_idx: usize = 0;
    while (run_idx < 5) : (run_idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "post_improve_{d}", .{run_idx});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_improve", "agent_improve", "user_improve", &state, 8);
    }

    var tagged_q = try db_ctx.conn.query(
        \\SELECT COUNT(*)
        \\FROM agent_run_scores
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6aa1'
    , .{});
    const tagged_row = (try tagged_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 5), tagged_row.get(i64, 0) catch -1);
    _ = tagged_q.next() catch {};
    tagged_q.deinit();

    var delta_q = try db_ctx.conn.query(
        \\SELECT score_delta
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6aa1'
    , .{});
    const delta_row = (try delta_q.next()) orelse return error.TestUnexpectedResult;
    const score_delta = delta_row.get(?f64, 0) catch null;
    try std.testing.expect(score_delta != null);
    try std.testing.expect(score_delta.? > 50.0);
    _ = delta_q.next() catch {};
    delta_q.deinit();
}

test "scoreRunIfTerminal resets trust after three consecutive negative score deltas" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    try insertScoringWorkspace(db_ctx.conn, "ws_stalled");
    try insertAgentProfile(db_ctx.conn, "agent_stalled", "ws_stalled", 10, "TRUSTED");

    var hist_idx: usize = 0;
    while (hist_idx < 5) : (hist_idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "hist_stalled_{d}", .{hist_idx});
        defer std.testing.allocator.free(run_id);
        try insertHistoricalScore(db_ctx.conn, "agent_stalled", "ws_stalled", run_id, 90, @intCast(hist_idx + 1), null, false);
    }

    const proposals_to_seed = [_][]const u8{
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ab1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ab2",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ab3",
    };
    for (proposals_to_seed, 0..) |proposal_id, idx| {
        const applied_at: i64 = @intCast(100 + idx);
        _ = try db_ctx.conn.exec(
            \\INSERT INTO agent_improvement_proposals
            \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, created_at, updated_at)
            \\VALUES ($1, 'agent_stalled', 'ws_stalled', 'DECLINING_SCORE', '[]', 'cfg_x', 'MANUAL', 'READY', 'APPLIED', 'operator:test', $2, $2)
        , .{ proposal_id, applied_at });
        _ = try db_ctx.conn.exec(
            \\INSERT INTO harness_change_log
            \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from, score_delta)
            \\VALUES ($1, 'agent_stalled', $2, 'ws_stalled', 'stage_insert', '{}', '{}', $3, 'operator:test', NULL, -5.0)
        , .{ proposal_id, proposal_id, applied_at });
    }

    const bad_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 2 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_stalled_final", "ws_stalled", "agent_stalled", "user_stalled", &bad_state, 30);

    try expectTrustState(db_ctx.conn, "agent_stalled", 0, "UNEARNED");
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

test "persistRunAnalysis classifies infra failure classes from error names" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);

    const cases = [_]struct {
        run_id: []const u8,
        error_name: []const u8,
        expected_class: []const u8,
    }{
        .{ .run_id = "run_deadline_cls", .error_name = "RunDeadlineExceeded", .expected_class = "TIMEOUT" },
        .{ .run_id = "run_timeout_cls", .error_name = "CommandTimedOut", .expected_class = "TIMEOUT" },
        .{ .run_id = "run_oom_cls", .error_name = "OutOfMemory", .expected_class = "OOM" },
        .{ .run_id = "run_context_cls", .error_name = "ContextWindowExceeded", .expected_class = "CONTEXT_OVERFLOW" },
        .{ .run_id = "run_auth_cls", .error_name = "TokenExpired", .expected_class = "AUTH_FAILURE" },
    };

    for (cases) |case| {
        const state = scoring.ScoringState{
            .outcome = .error_propagation,
            .stages_passed = 0,
            .stages_total = 1,
            .failure_error_name = case.error_name,
        };
        try scoring.persistRunAnalysis(db_ctx.conn, std.testing.allocator, case.run_id, "ws_cls_infra", "agent_cls_infra", &state, 0, 1, 1);
    }

    for (cases) |case| {
        var q = try db_ctx.conn.query("SELECT failure_class, failure_is_infra FROM agent_run_analysis WHERE run_id = $1", .{case.run_id});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(case.expected_class, row.get([]const u8, 0) catch "");
        try std.testing.expectEqual(true, row.get(bool, 1) catch false);
        q.drain() catch {};
    }
}

test "persistRunAnalysis classifies agent-attributable and unknown failures" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);

    const tool_state = scoring.ScoringState{
        .outcome = .error_propagation,
        .stages_passed = 0,
        .stages_total = 1,
        .failure_error_name = "CommandFailed",
    };
    try scoring.persistRunAnalysis(db_ctx.conn, std.testing.allocator, "run_tool_cls", "ws_cls_agent", "agent_cls_agent", &tool_state, 0, 1, 1);

    const bad_output_state = scoring.ScoringState{
        .outcome = .blocked_stage_graph,
        .stages_passed = 0,
        .stages_total = 1,
    };
    try scoring.persistRunAnalysis(db_ctx.conn, std.testing.allocator, "run_bad_output_cls", "ws_cls_agent", "agent_cls_agent", &bad_output_state, 0, 1, 1);

    const unknown_state = scoring.ScoringState{
        .outcome = .pending,
        .stages_passed = 0,
        .stages_total = 1,
    };
    try scoring.persistRunAnalysis(db_ctx.conn, std.testing.allocator, "run_unknown_cls", "ws_cls_agent", "agent_cls_agent", &unknown_state, 0, 1, 1);

    const cases = [_]struct {
        run_id: []const u8,
        expected_class: []const u8,
    }{
        .{ .run_id = "run_tool_cls", .expected_class = "TOOL_CALL_FAILURE" },
        .{ .run_id = "run_bad_output_cls", .expected_class = "BAD_OUTPUT_FORMAT" },
        .{ .run_id = "run_unknown_cls", .expected_class = "UNKNOWN" },
    };

    for (cases) |case| {
        var q = try db_ctx.conn.query("SELECT failure_class, failure_is_infra FROM agent_run_analysis WHERE run_id = $1", .{case.run_id});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(case.expected_class, row.get([]const u8, 0) catch "");
        try std.testing.expectEqual(false, row.get(bool, 1) catch true);
        q.drain() catch {};
    }
}

test "persistRunAnalysis scrubs stderr tail secrets before storage" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);

    const state = scoring.ScoringState{
        .outcome = .error_propagation,
        .stages_passed = 0,
        .stages_total = 1,
        .failure_error_name = "CommandFailed",
        .stderr_tail =
            "API_KEY=secret\n" ++
            "Bearer topsecret\n" ++
            "DATABASE_URL_API=postgres://secret\n" ++
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\n",
    };
    try scoring.persistRunAnalysis(db_ctx.conn, std.testing.allocator, "run_scrubbed_tail", "ws_scrub", "agent_scrub", &state, 0, 1, 1);

    var q = try db_ctx.conn.query("SELECT stderr_tail FROM agent_run_analysis WHERE run_id = 'run_scrubbed_tail'", .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const stderr_tail = row.get(?[]const u8, 0) catch null orelse return error.TestUnexpectedResult;
    try std.testing.expect(!std.mem.containsAtLeast(u8, stderr_tail, 1, "secret"));
    try std.testing.expect(std.mem.containsAtLeast(u8, stderr_tail, 1, "[REDACTED]"));
    q.drain() catch {};
}

test "buildScoringContextForEcho respects configured token cap via runtime estimator" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('ent_ctx_3', 'ws_ctx_3', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', true, 512, 0, 0)
    , .{});

    try insertHistoricalScore(db_ctx.conn, "agent_ctx_3", "ws_ctx_3", "run_ctx_3_1", 10, 1, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_ctx_3", "ws_ctx_3", "run_ctx_3_2", 10, 2, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_ctx_3", "ws_ctx_3", "run_ctx_3_3", 10, 3, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_ctx_3", "ws_ctx_3", "run_ctx_3_4", 10, 4, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_ctx_3", "ws_ctx_3", "run_ctx_3_5", 10, 5, "TIMEOUT", true);

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_ctx_3");
    const block = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_ctx_3", "agent_ctx_3", cfg);
    defer std.testing.allocator.free(block);

    try std.testing.expect(scoring.estimateScoringContextTokens(block) <= 512);
}

test "integration: M9_003 demo scoring context reduces timeout rate over ten guided runs" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempScoringTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
        \\VALUES ('ent_demo_ctx', 'ws_demo_ctx', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', true, 2048, 0, 0)
    , .{});

    try insertHistoricalScore(db_ctx.conn, "agent_demo_ctx", "ws_demo_ctx", "run_demo_hist_1", 20, 1, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_demo_ctx", "ws_demo_ctx", "run_demo_hist_2", 20, 2, "TIMEOUT", true);
    try insertHistoricalScore(db_ctx.conn, "agent_demo_ctx", "ws_demo_ctx", "run_demo_hist_3", 20, 3, "TIMEOUT", true);

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_demo_ctx");
    const injected_context = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_demo_ctx", "agent_demo_ctx", cfg);
    defer std.testing.allocator.free(injected_context);

    const baseline_timeouts = try runTimeoutDemoCohort("");
    const injected_timeouts = try runTimeoutDemoCohort(injected_context);

    std.debug.print(
        "M9_003 demo evidence baseline_timeouts={d} injected_timeouts={d}\n",
        .{ baseline_timeouts, injected_timeouts },
    );

    try std.testing.expectEqual(@as(u32, 6), baseline_timeouts);
    try std.testing.expectEqual(@as(u32, 2), injected_timeouts);
    try std.testing.expect(injected_timeouts < baseline_timeouts);
}
