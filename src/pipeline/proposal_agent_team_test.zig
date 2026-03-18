const std = @import("std");
const pg = @import("pg");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");

fn execSql(conn: *pg.Conn, sql: []const u8) !void {
    _ = try conn.exec(sql, .{});
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
        \\)
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_profiles (
        \\  agent_id TEXT PRIMARY KEY,
        \\  workspace_id TEXT NOT NULL,
        \\  trust_streak_runs INTEGER NOT NULL,
        \\  trust_level TEXT NOT NULL,
        \\  last_scored_at BIGINT,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY,
        \\  agent_id TEXT NOT NULL,
        \\  compiled_profile_json TEXT
        \\)
    );
    try execSql(conn,
        \\CREATE TEMP TABLE workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  config_version_id TEXT NOT NULL
        \\)
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
        \\)
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
        \\)
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
        \\)
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
        \\)
    );
}

fn insertAgentProfile(conn: *pg.Conn, agent_id: []const u8, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_profiles (agent_id, workspace_id, trust_streak_runs, trust_level, last_scored_at, updated_at)
        \\VALUES ($1, $2, 0, 'UNEARNED', NULL, 0)
    , .{ agent_id, workspace_id });
}

fn insertActiveConfigWithProfile(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    profile_json: []const u8,
) !void {
    // check-pg-drain: ok — no conn.query() here; checker misattributes test-block queries
    _ = try conn.exec(
        \\INSERT INTO agent_config_versions (config_version_id, agent_id, compiled_profile_json)
        \\VALUES ($1, $2, $3)
    , .{ config_version_id, agent_id, profile_json });
    _ = try conn.exec(
        \\INSERT INTO workspace_active_config (workspace_id, config_version_id)
        \\VALUES ($1, $2)
    , .{ workspace_id, config_version_id });
}

test "reconcilePendingProposalGenerations preserves dynamic auto-agent gate role and skill" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_team_1', 'ws_prop_team_1', 'SCALE', 6, 10, 10, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try insertAgentProfile(db_ctx.conn, "agent_prop_team_1", "ws_prop_team_1");
    try insertActiveConfigWithProfile(db_ctx.conn, "agent_prop_team_1", "ws_prop_team_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f97", "{\"agent_id\":\"agent-team\",\"stages\":[{\"stage_id\":\"autoforecaster\",\"role\":\"autoforecaster\",\"skill\":\"clawhub://usezombie/autoforecaster@1.0.0\"},{\"stage_id\":\"autoprocurer\",\"role\":\"autoprocurer\",\"skill\":\"clawhub://usezombie/autoprocurer@1.0.0\"},{\"stage_id\":\"autoworkerstandup\",\"role\":\"autoworkerstandup\",\"skill\":\"clawhub://usezombie/autoworkerstandup@1.0.0\"},{\"stage_id\":\"autoworkerready\",\"role\":\"autoworkerready\",\"skill\":\"clawhub://usezombie/autoworkerready@1.0.0\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 4 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_team_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "ws_prop_team_1", "agent_prop_team_1", "user_prop_team_1", &low_state, 20);
    }

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), result.ready);
    try std.testing.expectEqual(@as(u32, 0), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_team_1'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const proposed_changes = row.get([]const u8, 0) catch "";
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"insert_before_stage_id\":\"autoworkerready\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"stage_id\":\"autoworkerready-precheck\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"role\":\"autoworkerready\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, proposed_changes, 1, "\"skill\":\"clawhub://usezombie/autoworkerready@1.0.0\""));
    try std.testing.expect(!std.mem.containsAtLeast(u8, proposed_changes, 1, "\"role\":\"warden\""));
    try std.testing.expect((try q.next()) == null);
}
