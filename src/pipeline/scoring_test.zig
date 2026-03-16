const std = @import("std");
const pg = @import("pg");
const metrics = @import("../observability/metrics.zig");
const scoring = @import("scoring.zig");

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
            \\  enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE,
            \\  scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
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

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 1, .stages_total = 1 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_1", "ws_1", "agent_1", "user_1", &state, 12);

    const after_body = try metrics.renderPrometheus(std.testing.allocator, false, 0, 0);
    defer std.testing.allocator.free(after_body);
    const after_failed = try prometheusMetricValue(after_body, "zombie_agent_scoring_failed_total");

    try std.testing.expectEqual(before_failed + 1, after_failed);
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

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    scoring.scoreRunIfTerminal(db_ctx.conn, null, "run_3", "ws_3", "agent_3", "user_3", &state, 8);

    {
        var q = try db_ctx.conn.query("SELECT score, agent_id FROM agent_run_scores WHERE run_id = 'run_3'", .{});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i32, 95), row.get(i32, 0) catch -1);
        try std.testing.expectEqualStrings("agent_3", row.get([]const u8, 1) catch "");
    }
    {
        var q = try db_ctx.conn.query("SELECT failure_class, failure_is_infra FROM agent_run_analysis WHERE run_id = 'run_3'", .{});
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expect((row.get(?[]const u8, 0) catch null) == null);
        try std.testing.expectEqual(false, row.get(bool, 1) catch true);
    }
}

test "buildScoringContextForEcho returns orientation when no history" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempScoringTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
            \\VALUES ('ent_ctx_1', 'ws_ctx_1', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', true, 2048, 0, 0)
        , .{});
        q.deinit();
    }

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_ctx_1");
    const block = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_ctx_1", "agent_ctx_1", cfg);
    defer std.testing.allocator.free(block);

    try std.testing.expect(std.mem.containsAtLeast(u8, block, 1, "You have no prior score history"));
}

test "buildScoringContextForEcho is empty when injection disabled" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempScoringTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, enable_score_context_injection, scoring_context_max_tokens, created_at, updated_at)
            \\VALUES ('ent_ctx_2', 'ws_ctx_2', 'FREE', 1, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', false, 2048, 0, 0)
        , .{});
        q.deinit();
    }

    const cfg = try scoring.queryScoringConfig(db_ctx.conn, std.testing.allocator, "ws_ctx_2");
    const block = try scoring.buildScoringContextForEcho(db_ctx.conn, std.testing.allocator, "ws_ctx_2", "agent_ctx_2", cfg);
    defer std.testing.allocator.free(block);

    try std.testing.expectEqual(@as(usize, 0), block.len);
}
