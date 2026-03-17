const std = @import("std");
const pg = @import("pg");
const topology = @import("topology.zig");

fn execSql(conn: *pg.Conn, sql: []const u8) !void {
    // Rule 1: exec() for DDL/DML — internal drain loop, always leaves _state=.idle
    _ = try conn.exec(sql, .{});
}

pub fn createTempProposalTables(conn: *pg.Conn) !void {
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
        \\CREATE TEMP TABLE workspace_latency_baseline (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  p50_seconds BIGINT NOT NULL,
        \\  p95_seconds BIGINT NOT NULL,
        \\  sample_count INTEGER NOT NULL,
        \\  computed_at BIGINT NOT NULL
        \\)
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
        \\)
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL,
        \\  agent_id TEXT NOT NULL,
        \\  version INTEGER NOT NULL,
        \\  source_markdown TEXT NOT NULL,
        \\  compiled_profile_json TEXT,
        \\  compile_engine TEXT NOT NULL,
        \\  validation_report_json TEXT NOT NULL DEFAULT '{}',
        \\  is_valid BOOLEAN NOT NULL DEFAULT FALSE,
        \\  created_at BIGINT NOT NULL DEFAULT 0,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\)
    );
    try execSql(conn,
        \\CREATE TEMP TABLE workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  tenant_id TEXT NOT NULL DEFAULT 'tenant_test',
        \\  config_version_id TEXT NOT NULL,
        \\  activated_by TEXT NOT NULL DEFAULT 'test',
        \\  activated_at BIGINT NOT NULL DEFAULT 0
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
        \\  proposal_id TEXT,
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
    try execSql(conn,
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
    );
}

pub fn insertAgentProfile(conn: *pg.Conn, agent_id: []const u8, workspace_id: []const u8) !void {
    try insertAgentProfileWithTrust(conn, agent_id, workspace_id, 0, "UNEARNED");
}

pub fn insertAgentProfileWithTrust(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    trust_streak_runs: i32,
    trust_level: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_profiles
        \\  (agent_id, tenant_id, workspace_id, name, status, trust_streak_runs, trust_level, last_scored_at, created_at, updated_at)
        \\VALUES ($1, 'tenant_test', $2, 'Agent', 'ACTIVE', $3, $4, NULL, 0, 0)
    , .{ agent_id, workspace_id, trust_streak_runs, trust_level });
}

pub fn insertActiveConfig(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
) !void {
    try insertActiveConfigWithProfile(conn, agent_id, workspace_id, config_version_id, "{\"agent_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"implement\",\"role\":\"scout\",\"skill\":\"scout\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}");
}

pub fn insertActiveConfigWithProfile(
    conn: *pg.Conn,
    agent_id: []const u8,
    workspace_id: []const u8,
    config_version_id: []const u8,
    profile_json: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown, compiled_profile_json,
        \\   compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, 'tenant_test', $2, 1, $3, $3, 'deterministic-v1', '{}', TRUE, 0, 0)
    , .{ config_version_id, agent_id, profile_json });
    _ = try conn.exec(
        \\INSERT INTO workspace_active_config (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
        \\VALUES ($1, 'tenant_test', $2, 'test', 0)
    , .{ workspace_id, config_version_id });
}

pub fn insertConfigVersionOnly(
    conn: *pg.Conn,
    agent_id: []const u8,
    config_version_id: []const u8,
    version: i32,
    profile_json: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown, compiled_profile_json,
        \\   compile_engine, validation_report_json, is_valid, created_at, updated_at)
        \\VALUES ($1, 'tenant_test', $2, $3, $4, $4, 'deterministic-v1', '{}', TRUE, 0, 0)
    , .{ config_version_id, agent_id, version, profile_json });
}

pub fn insertScoreRow(
    conn: *pg.Conn,
    run_id: []const u8,
    agent_id: []const u8,
    workspace_id: []const u8,
    score: i32,
    scored_at: i64,
) !void {
    _ = try conn.exec(
        \\INSERT INTO agent_run_scores
        \\  (score_id, run_id, agent_id, workspace_id, score, axis_scores, weight_snapshot, scored_at)
        \\VALUES ($1, $2, $3, $4, $5, '{}', '{}', $6)
    , .{ run_id, run_id, agent_id, workspace_id, score, scored_at });
}

pub const ExpectedStage = struct {
    stage_id: []const u8,
    role_id: []const u8,
    skill_id: []const u8,
    artifact_name: []const u8,
    commit_message: []const u8,
    is_gate: bool,
    on_pass: ?[]const u8,
    on_fail: ?[]const u8,
};

pub fn expectProfileMatches(raw_json: []const u8, expected_agent_id: []const u8, expected_stages: []const ExpectedStage) !void {
    var parsed = try topology.parseProfileJson(std.testing.allocator, raw_json);
    defer parsed.deinit();

    try std.testing.expectEqualStrings(expected_agent_id, parsed.agent_id);
    try std.testing.expectEqual(expected_stages.len, parsed.stages.len);

    for (expected_stages, parsed.stages) |expected_stage, actual_stage| {
        try std.testing.expectEqualStrings(expected_stage.stage_id, actual_stage.stage_id);
        try std.testing.expectEqualStrings(expected_stage.role_id, actual_stage.role_id);
        try std.testing.expectEqualStrings(expected_stage.skill_id, actual_stage.skill_id);
        try std.testing.expectEqualStrings(expected_stage.artifact_name, actual_stage.artifact_name);
        try std.testing.expectEqualStrings(expected_stage.commit_message, actual_stage.commit_message);
        try std.testing.expectEqual(expected_stage.is_gate, actual_stage.is_gate);
        try std.testing.expectEqualStrings(expected_stage.on_pass orelse "", actual_stage.on_pass orelse "");
        try std.testing.expectEqualStrings(expected_stage.on_fail orelse "", actual_stage.on_fail orelse "");
    }
}
