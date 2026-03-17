const std = @import("std");
const pg = @import("pg");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");
const proposals_shared = @import("scoring_mod/proposals_shared.zig");

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

fn execSql(conn: *pg.Conn, sql: []const u8) !void {
    var q = try conn.query(sql, .{});
    q.deinit();
}

fn createTempProposalTables(conn: *pg.Conn) !void {
    try execSql(conn,
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
    );
    try execSql(conn,
        \\CREATE TEMP TABLE workspace_latency_baseline (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  p50_seconds BIGINT NOT NULL,
        \\  p95_seconds BIGINT NOT NULL,
        \\  sample_count INTEGER NOT NULL,
        \\  computed_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_profiles (
        \\  agent_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  workspace_id TEXT NOT NULL,
        \\  name TEXT NOT NULL DEFAULT 'Agent',
        \\  status TEXT NOT NULL DEFAULT 'DRAFT',
        \\  trust_streak_runs INTEGER NOT NULL,
        \\  trust_level TEXT NOT NULL,
        \\  last_scored_at BIGINT,
        \\  created_at BIGINT NOT NULL DEFAULT 0,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY,
        \\  agent_id TEXT NOT NULL,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  version INTEGER NOT NULL DEFAULT 1,
        \\  source_markdown TEXT NOT NULL DEFAULT '{}',
        \\  compiled_profile_json TEXT,
        \\  compile_engine TEXT,
        \\  validation_report_json TEXT NOT NULL DEFAULT '{}',
        \\  is_valid BOOLEAN NOT NULL DEFAULT TRUE,
        \\  created_at BIGINT NOT NULL DEFAULT 0,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    );
    try execSql(conn,
        \\CREATE TEMP TABLE workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  config_version_id TEXT NOT NULL,
        \\  activated_by TEXT NOT NULL DEFAULT 'test',
        \\  activated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    );
    try execSql(conn,
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
    );
    try execSql(conn,
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
    );
    try execSql(conn,
        \\CREATE TEMP TABLE entitlement_policy_audit_snapshots (
        \\  snapshot_id TEXT PRIMARY KEY,
        \\  workspace_id TEXT NOT NULL,
        \\  boundary TEXT NOT NULL,
        \\  decision TEXT NOT NULL,
        \\  reason_code TEXT NOT NULL,
        \\  plan_tier TEXT NOT NULL,
        \\  policy_json TEXT NOT NULL,
        \\  observed_json TEXT NOT NULL,
        \\  actor TEXT NOT NULL,
        \\  created_at BIGINT NOT NULL
        \\) ON COMMIT DROP
    );
    try execSql(conn,
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
        \\) ON COMMIT DROP
    );
}

fn insertAgentProfile(conn: *pg.Conn, agent_id: []const u8, workspace_id: []const u8) !void {
    try insertAgentProfileWithTrust(conn, agent_id, workspace_id, 0, "UNEARNED");
}

fn insertAgentProfileWithTrust(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    trust_streak_runs: i32,
    trust_level: []const u8,
) !void {
    var q = try conn.query(
        \\INSERT INTO agent_profiles
        \\  (agent_id, tenant_id, workspace_id, name, status, trust_streak_runs, trust_level, last_scored_at, created_at, updated_at)
        \\VALUES ($1, 'tenant_test', $2, 'Agent', 'ACTIVE', $3, $4, NULL, 0, 0)
    , .{ agent_id, workspace_id, trust_streak_runs, trust_level });
    q.deinit();
}

fn insertActiveConfig(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
) !void {
    try insertActiveConfigWithProfile(conn, agent_id, workspace_id, config_version_id,
        "{\"profile_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"implement\",\"role\":\"scout\",\"skill\":\"scout\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}"
    );
}

fn insertActiveConfigWithProfile(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    profile_json: []const u8,
) !void {
    {
        var q = try conn.query(
            \\INSERT INTO agent_config_versions
            \\  (config_version_id, agent_id, tenant_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, created_at, updated_at)
            \\VALUES ($1, $2, 'tenant_test', 1, $3, $3, 'deterministic-v1', '{}', TRUE, 0, 0)
        , .{ config_version_id, agent_id, profile_json });
        q.deinit();
    }
    {
        var q = try conn.query(
            \\INSERT INTO workspace_active_config (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
            \\VALUES ($1, 'tenant_test', $2, 'test', 0)
        , .{ workspace_id, config_version_id });
        q.deinit();
    }
}

fn insertConfigVersionOnly(
    conn: *pg.Conn,
    agent_id: []const u8,
    config_version_id: []const u8,
    version: i32,
    profile_json: []const u8,
) !void {
    var q = try conn.query(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, agent_id, tenant_id, version, source_markdown, compiled_profile_json, compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, $2, 'tenant_test', $3, $4, $4, 'deterministic-v1', '{}', TRUE, 0, 0)
    , .{ config_version_id, agent_id, version, profile_json });
    q.deinit();
}

fn insertScoreRow(
    conn: *pg.Conn,
    run_id: []const u8,
    agent_id: []const u8,
    workspace_id: []const u8,
    score: i32,
    scored_at: i64,
) !void {
    var q = try conn.query(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, '{}', '{}', $6)
    , .{ run_id, run_id, agent_id, workspace_id, score, scored_at });
    q.deinit();
}

test "scoreRunIfTerminal persists proposal groundwork after sustained low-score window" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_1', 'ws_prop_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfile(db_ctx.conn, "agent_prop_1", "ws_prop_1");
    try insertActiveConfig(db_ctx.conn, "agent_prop_1", "ws_prop_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_low_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_prop_1", "agent_prop_1", "user_prop_1", &low_state, 20);
    }

    var q = try db_ctx.conn.query(
        \\SELECT trigger_reason, proposed_changes, approval_mode, generation_status, status, config_version_id, auto_apply_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_1'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("SUSTAINED_LOW_SCORE", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("[]", row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("MANUAL", row.get([]const u8, 2) catch "");
    try std.testing.expectEqualStrings("PENDING", row.get([]const u8, 3) catch "");
    try std.testing.expectEqualStrings("PENDING_REVIEW", row.get([]const u8, 4) catch "");
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92", row.get([]const u8, 5) catch "");
    try std.testing.expect((row.get(?i64, 6) catch null) == null);
    try std.testing.expect((try q.next()) == null);
}

test "scoreRunIfTerminal triggers proposal on declining five-run average" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_2', 'ws_prop_2', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_latency_baseline
            \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
            \\VALUES ('ws_prop_2', 10, 30, 5, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfile(db_ctx.conn, "agent_prop_2", "ws_prop_2");
    try insertActiveConfig(db_ctx.conn, "agent_prop_2", "ws_prop_2", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93");

    const high_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_high_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_prop_2", "agent_prop_2", "user_prop_2", &high_state, 8);
    }

    const medium_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 1, .stages_total = 2 };
    i = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_med_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_prop_2", "agent_prop_2", "user_prop_2", &medium_state, 20);
    }

    var q = try db_ctx.conn.query(
        \\SELECT trigger_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_2'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("DECLINING_SCORE", row.get([]const u8, 0) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "trusted proposal enters veto window with auto-apply deadline" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_trusted_1', 'ws_prop_trusted_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_trusted_1", "ws_prop_trusted_1", 10, "TRUSTED");
    try insertActiveConfig(db_ctx.conn, "agent_prop_trusted_1", "ws_prop_trusted_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa1");

    var ts: i64 = 1_000;
    while (ts < 6_000) : (ts += 1_000) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prev_{d}", .{ts});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_trusted_1", "ws_prop_trusted_1", 95, ts);
    }
    while (ts < 11_000) : (ts += 1_000) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_curr_{d}", .{ts});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_trusted_1", "ws_prop_trusted_1", 80, ts);
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "ws_prop_trusted_1", "agent_prop_trusted_1", 11_000);

    var q = try db_ctx.conn.query(
        \\SELECT approval_mode, status, auto_apply_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_trusted_1'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("AUTO", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("VETO_WINDOW", row.get([]const u8, 1) catch "");
    try std.testing.expectEqual(@as(i64, 11_000 + proposals_shared.AUTO_APPLY_WINDOW_MS), row.get(?i64, 2) catch null orelse -1);
}

test "reconcilePendingProposalGenerations materializes generated stage proposal payload" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_4', 'ws_prop_4', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfile(db_ctx.conn, "agent_prop_4", "ws_prop_4");
    try insertActiveConfig(db_ctx.conn, "agent_prop_4", "ws_prop_4", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_ready_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_prop_4", "agent_prop_4", "user_prop_4", &low_state, 20);
    }

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), result.ready);
    try std.testing.expectEqual(@as(u32, 0), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes, generation_status, status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_4'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const proposed_changes = row.get([]const u8, 0) catch "";
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"target_field\":\"stage_insert\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"insert_before_stage_id\":\"verify\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"stage_id\":\"verify-precheck\""));
    try std.testing.expectEqualStrings("READY", row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("PENDING_REVIEW", row.get([]const u8, 2) catch "");
    try std.testing.expect((row.get(?[]const u8, 3) catch null) == null);
    try std.testing.expect((try q.next()) == null);
}

test "reconcileDueAutoApprovalProposals applies overdue veto-window proposals" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_auto_1', 'ws_prop_auto_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_auto_1", "ws_prop_auto_1", 10, "TRUSTED");
    try insertActiveConfig(db_ctx.conn, "agent_prop_auto_1", "ws_prop_auto_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_prev_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_1", "ws_prop_auto_1", 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_curr_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_1", "ws_prop_auto_1", 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "ws_prop_auto_1", "agent_prop_auto_1", 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    const now_ms: i64 = 20_000;
    var due_q = try db_ctx.conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1
    , .{ "agent_prop_auto_1", now_ms });
    due_q.deinit();

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, now_ms + 1);
    try std.testing.expectEqual(@as(u32, 1), result.applied);
    try std.testing.expectEqual(@as(u32, 0), result.config_changed);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, applied_by
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_auto_1'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("APPLIED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("system:auto", proposal_row.get([]const u8, 1) catch "");

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_auto_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2", active_row.get([]const u8, 0) catch ""));
}

test "reconcileDueAutoApprovalProposals rejects auto-apply when config version changed" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_auto_2', 'ws_prop_auto_2', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_auto_2", "ws_prop_auto_2", 10, "TRUSTED");
    try insertActiveConfig(db_ctx.conn, "agent_prop_auto_2", "ws_prop_auto_2", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa3");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_cfg_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_2", "ws_prop_auto_2", 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_cfg_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_2", "ws_prop_auto_2", 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "ws_prop_auto_2", "agent_prop_auto_2", 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    try insertConfigVersionOnly(
        db_ctx.conn,
        "agent_prop_auto_2",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4",
        2,
        "{\"profile_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}"
    );
    var flip_q = try db_ctx.conn.query(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4'
        \\WHERE workspace_id = 'ws_prop_auto_2'
    , .{});
    flip_q.deinit();

    const due_ms: i64 = 20_000;
    var due_q = try db_ctx.conn.query(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1
    , .{ "agent_prop_auto_2", due_ms });
    due_q.deinit();

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, due_ms + 1);
    try std.testing.expectEqual(@as(u32, 0), result.applied);
    try std.testing.expectEqual(@as(u32, 1), result.config_changed);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_auto_2'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("CONFIG_CHANGED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("CONFIG_CHANGED_SINCE_PROPOSAL", proposal_row.get([]const u8, 1) catch "");
}

test "reconcilePendingProposalGenerations rejects generated proposals that exceed stage entitlements" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_5', 'ws_prop_5', 'FREE', 3, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfile(db_ctx.conn, "agent_prop_5", "ws_prop_5");
    try insertActiveConfig(db_ctx.conn, "agent_prop_5", "ws_prop_5", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95");
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO agent_improvement_proposals
            \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
            \\VALUES ('prop_stage_limit_1', 'agent_prop_5', 'ws_prop_5', 'SUSTAINED_LOW_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95', 'MANUAL', 'PENDING', 'PENDING_REVIEW', NULL, 0, 0)
        , .{});
        q.deinit();
    }

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 0), result.ready);
    try std.testing.expectEqual(@as(u32, 1), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes, generation_status, status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = 'prop_stage_limit_1'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("[]", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 2) catch "");
    try std.testing.expectEqualStrings("UZ-ENTL-003", row.get([]const u8, 3) catch "");
}

test "proposal validation rejects unregistered agent refs and entitlement-disallowed skills" {
    const db_ctx = (try openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try createTempProposalTables(db_ctx.conn);
    {
        var q = try db_ctx.conn.query(
            \\INSERT INTO workspace_entitlements
            \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
            \\VALUES ('ent_prop_3', 'ws_prop_3', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        , .{});
        q.deinit();
    }
    try insertAgentProfile(db_ctx.conn, "agent_prop_3", "ws_prop_3");
    try insertActiveConfig(db_ctx.conn, "agent_prop_3", "ws_prop_3", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f96");

    try std.testing.expectError(
        proposals.ProposalValidationError.UnregisteredAgentRef,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            "ws_prop_3",
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"missing-agent\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\"},\"rationale\":\"rebind stage\"}]",
        ),
    );

    try std.testing.expectError(
        proposals.ProposalValidationError.EntitlementSkillNotAllowed,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            "ws_prop_3",
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"agent_prop_3\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"clawhub://openclaw/github-reviewer@1.2.0\"},\"rationale\":\"rebind stage\"}]",
        ),
    );
}
