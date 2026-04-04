//! M27_002 billing gate — scoring integration tests.
//!
//! Tests `scoreRunForBillingGate` end-to-end using the scoring temp-table harness.
//! Verifies per-tier gate decisions, spec §1.1.3 (additive-only), and §1.1.4
//! (unscored runs follow normal billing path).
//!
//! Requires DATABASE_URL — skipped when absent.

const std = @import("std");
const pg = @import("pg");
const scoring = @import("scoring.zig");
const common = @import("../http/handlers/common.zig");

// ── Workspace / agent IDs (prefix bg → billing-gate) ────────────────────────
// Offset from scoring_test.zig IDs (which go up to cc14) by using cc20+ here.
const WS_BG = "0195b4ba-8d3a-7f13-8abc-cc0000000020";
const AGENT_BRONZE = "agent_bg_bronze";
const AGENT_SILVER = "agent_bg_silver";
const AGENT_GOLD = "agent_bg_gold";
const AGENT_ELITE = "agent_bg_elite";
const AGENT_DISABLED = "agent_bg_disabled"; // scoring disabled workspace
const AGENT_EMPTY_ID = ""; // no profile

// ── Temp table helpers (replicated from scoring_test.zig scope) ──────────────

fn createScoringTempTables(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS workspace_entitlements (
        \\  entitlement_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL UNIQUE,
        \\  plan_tier TEXT NOT NULL, max_profiles INTEGER NOT NULL,
        \\  max_stages INTEGER NOT NULL, max_distinct_skills INTEGER NOT NULL,
        \\  allow_custom_skills BOOLEAN NOT NULL, enable_agent_scoring BOOLEAN NOT NULL DEFAULT FALSE,
        \\  agent_scoring_weights_json TEXT NOT NULL DEFAULT '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\  enable_score_context_injection BOOLEAN NOT NULL DEFAULT TRUE,
        \\  scoring_context_max_tokens INTEGER NOT NULL DEFAULT 2048,
        \\  created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS workspace_latency_baseline (
        \\  workspace_id TEXT PRIMARY KEY, p50_seconds BIGINT NOT NULL,
        \\  p95_seconds BIGINT NOT NULL, sample_count INTEGER NOT NULL, computed_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS agent_profiles (
        \\  agent_id TEXT PRIMARY KEY, workspace_id TEXT NOT NULL,
        \\  status TEXT NOT NULL DEFAULT 'ACTIVE', trust_streak_runs INTEGER NOT NULL DEFAULT 0,
        \\  trust_level TEXT NOT NULL DEFAULT 'UNEARNED',
        \\  last_scored_at BIGINT, updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS agent_run_analysis (
        \\  analysis_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE,
        \\  agent_id TEXT NOT NULL, workspace_id TEXT NOT NULL,
        \\  failure_class TEXT, failure_is_infra BOOLEAN NOT NULL DEFAULT FALSE,
        \\  failure_signals JSONB NOT NULL DEFAULT '[]'::jsonb,
        \\  improvement_hints JSONB NOT NULL DEFAULT '[]'::jsonb,
        \\  stderr_tail TEXT, analyzed_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS agent_run_scores (
        \\  score_id TEXT PRIMARY KEY, run_id TEXT NOT NULL UNIQUE,
        \\  agent_id TEXT NOT NULL, workspace_id TEXT NOT NULL,
        \\  proposal_id TEXT, score INTEGER NOT NULL,
        \\  axis_scores TEXT NOT NULL, weight_snapshot TEXT NOT NULL, scored_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS agent_improvement_proposals (
        \\  proposal_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL,
        \\  workspace_id TEXT NOT NULL, trigger_reason TEXT NOT NULL,
        \\  proposed_changes TEXT NOT NULL, config_version_id TEXT NOT NULL,
        \\  approval_mode TEXT NOT NULL, generation_status TEXT NOT NULL,
        \\  status TEXT NOT NULL, rejection_reason TEXT, auto_apply_at BIGINT,
        \\  applied_by TEXT, created_at BIGINT NOT NULL, updated_at BIGINT NOT NULL
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS harness_change_log (
        \\  change_id TEXT PRIMARY KEY, agent_id TEXT NOT NULL,
        \\  proposal_id TEXT NOT NULL, workspace_id TEXT NOT NULL,
        \\  field_name TEXT NOT NULL, old_value TEXT NOT NULL, new_value TEXT NOT NULL,
        \\  applied_at BIGINT NOT NULL, applied_by TEXT NOT NULL,
        \\  reverted_from TEXT, score_delta DOUBLE PRECISION
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  agent_id TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1,
        \\  source_markdown TEXT NOT NULL DEFAULT '{}', compiled_profile_json TEXT,
        \\  compile_engine TEXT NOT NULL DEFAULT 'deterministic-v1',
        \\  validation_report_json TEXT NOT NULL DEFAULT '{}',
        \\  is_valid BOOLEAN NOT NULL DEFAULT FALSE,
        \\  created_at BIGINT NOT NULL DEFAULT 0, updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE IF NOT EXISTS workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY, tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  config_version_id TEXT NOT NULL, activated_by TEXT NOT NULL DEFAULT 'test',
        \\  activated_at BIGINT NOT NULL DEFAULT 0
        \\)
    , .{});
}

fn seedScoringWorkspaceEnabled(conn: *pg.Conn, ws: []const u8) !void {
    var ent_buf: [96]u8 = undefined;
    const ent = try std.fmt.bufPrint(&ent_buf, "ent_{s}", .{ws});
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages,
        \\   max_distinct_skills, allow_custom_skills, enable_agent_scoring,
        \\   agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, true,
        \\        '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{ ent, ws });
    _ = try conn.exec(
        "INSERT INTO workspace_latency_baseline (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at) VALUES ($1, 10, 30, 5, 0)",
        .{ws},
    );
}

fn seedScoringWorkspaceDisabled(conn: *pg.Conn, ws: []const u8) !void {
    var ent_buf: [96]u8 = undefined;
    const ent = try std.fmt.bufPrint(&ent_buf, "ent_{s}", .{ws});
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages,
        \\   max_distinct_skills, allow_custom_skills, enable_agent_scoring,
        \\   agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, false,
        \\        '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{ ent, ws });
}

fn seedAgentProfile(conn: *pg.Conn, agent: []const u8, ws: []const u8) !void {
    _ = try conn.exec(
        "INSERT INTO agent_profiles (agent_id, workspace_id, trust_streak_runs, trust_level, updated_at) VALUES ($1, $2, 0, 'UNEARNED', 0)",
        .{ agent, ws },
    );
}

fn expectScorePersistedFor(conn: *pg.Conn, run_id: []const u8) !i32 {
    var q = try conn.query("SELECT score FROM agent_run_scores WHERE run_id = $1", .{run_id});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedScoreRow;
    const s = try row.get(i32, 0);
    try q.drain();
    return s;
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

test "scoreRunForBillingGate returns null for pending outcome — unscored" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_BRONZE, WS_BG);

    const state = scoring.ScoringState{ .outcome = .pending };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_pending", WS_BG, AGENT_BRONZE, "user_bg", &state, 10);
    try std.testing.expectEqual(@as(?u8, null), result);
}

test "scoreRunForBillingGate returns null when scoring disabled — unscored" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);

    const ws_dis = "0195b4ba-8d3a-7f13-8abc-cc0000000021";
    try seedScoringWorkspaceDisabled(db_ctx.conn, ws_dis);
    try seedAgentProfile(db_ctx.conn, AGENT_DISABLED, ws_dis);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 3, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_disabled", ws_dis, AGENT_DISABLED, "user_bg", &state, 8);
    // Scoring disabled → null, run is not gated.
    try std.testing.expectEqual(@as(?u8, null), result);
}

test "scoreRunForBillingGate returns null for empty agent_id — spec §1.1.4" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 0, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_no_agent", WS_BG, AGENT_EMPTY_ID, "user_bg", &state, 10);
    try std.testing.expectEqual(@as(?u8, null), result);
}

// Bronze: blocked run, 0 stages — score < 40 → gate should trigger.
test "scoreRunForBillingGate Bronze: blocked, 0/3 stages — score below threshold" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_BRONZE, WS_BG);

    // blocked_stage_graph + 0/3 → score ≈ 19 (Bronze)
    const state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_bronze_1", WS_BG, AGENT_BRONZE, "user_bg", &state, 20);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? < scoring.BILLING_QUALITY_THRESHOLD);

    // Verify score was persisted to DB.
    const persisted = try expectScorePersistedFor(db_ctx.conn, "run_bg_bronze_1");
    try std.testing.expect(persisted < 40);
}

// Bronze: error propagation (code that crashed entirely) — score 0.
test "scoreRunForBillingGate Bronze: error_propagation — garbage run, score 0, gated" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_BRONZE, WS_BG);

    const state = scoring.ScoringState{ .outcome = .error_propagation, .stages_passed = 0, .stages_total = 2 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_bronze_2", WS_BG, AGENT_BRONZE, "user_bg", &state, 60);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? < scoring.BILLING_QUALITY_THRESHOLD);
}

// Silver: done run, code ran but 0 stages passed — score 55, NOT gated.
test "scoreRunForBillingGate Silver: done, 0/3 stages — score 55, not gated" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_SILVER, WS_BG);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 0, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_silver_1", WS_BG, AGENT_SILVER, "user_bg", &state, 10);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? >= scoring.BILLING_QUALITY_THRESHOLD);
}

// Silver: done run that took much longer than usual — higher latency lowers score.
test "scoreRunForBillingGate Silver: done, slow run 3x p50 — score 45, still billable" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_SILVER, WS_BG);

    // p50=10s baseline seeded. wall_seconds=30 → latency_score=0.
    // score = 100*0.4 + 0*0.3 + 0*0.2 + 50*0.1 = 40+0+0+5 = 45 — Silver, not gated.
    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 0, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_silver_slow", WS_BG, AGENT_SILVER, "user_bg", &state, 30);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? >= scoring.BILLING_QUALITY_THRESHOLD);
}

// Gold: done, 2/3 stages — score 75.
test "scoreRunForBillingGate Gold: done, 2/3 stages — score ≥70, billable" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_GOLD, WS_BG);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_gold_1", WS_BG, AGENT_GOLD, "user_bg", &state, 12);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? >= 70);
    try std.testing.expect(result.? >= scoring.BILLING_QUALITY_THRESHOLD);
}

// Elite: done, 3/3, fast run — score ≥90.
test "scoreRunForBillingGate Elite: done, 3/3, fast run — score ≥90, billable" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_ELITE, WS_BG);

    // wall_seconds=8 < p50=10 → latency_score=100; all stages pass → elite.
    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 3, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_elite_1", WS_BG, AGENT_ELITE, "user_bg", &state, 8);
    try std.testing.expect(result != null);
    try std.testing.expect(result.? >= 90);
}

// Elite false-positive: verify elite run is never accidentally gated.
test "scoreRunForBillingGate Elite: false-positive guard — elite score always above threshold" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_ELITE, WS_BG);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 3, .stages_total = 3 };
    const result = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_elite_2", WS_BG, AGENT_ELITE, "user_bg", &state, 5);
    try std.testing.expect(result != null);
    // Elite is always well above the 40 threshold — must never trigger gate.
    try std.testing.expect(result.? >= scoring.BILLING_QUALITY_THRESHOLD);
    try std.testing.expect(result.? >= 90);
}

// Idempotency: calling scoreRunForBillingGate twice on same run returns same score.
test "scoreRunForBillingGate is idempotent: double-call returns same score, no duplicate DB row" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    try createScoringTempTables(db_ctx.conn);
    try seedScoringWorkspaceEnabled(db_ctx.conn, WS_BG);
    try seedAgentProfile(db_ctx.conn, AGENT_GOLD, WS_BG);

    const state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 3 };
    const r1 = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_idem", WS_BG, AGENT_GOLD, "user_bg", &state, 10);
    const r2 = scoring.scoreRunForBillingGate(db_ctx.conn, null, "run_bg_idem", WS_BG, AGENT_GOLD, "user_bg", &state, 10);
    try std.testing.expectEqual(r1, r2);

    // Only one row must exist.
    var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM agent_run_scores WHERE run_id = 'run_bg_idem'", .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestExpectedRow;
    try std.testing.expectEqual(@as(i64, 1), try row.get(i64, 0));
    try q.drain();
}
