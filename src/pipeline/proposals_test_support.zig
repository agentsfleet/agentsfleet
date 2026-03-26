const std = @import("std");
const pg = @import("pg");
const topology = @import("topology.zig");

pub const base = @import("../db/test_fixtures.zig");

pub fn allocTestUuid(alloc: std.mem.Allocator, seed: u64) ![]u8 {
    return std.fmt.allocPrint(alloc, "0195b4ba-8d3a-7f13-8abc-{x:0>12}", .{seed});
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
        \\VALUES ($1, $2, $3, 'Agent', 'ACTIVE', $4, $5, NULL, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ agent_id, base.TEST_TENANT_ID, workspace_id, trust_streak_runs, trust_level });
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
        \\VALUES ($1, $2, $3, 1, $4, $4, 'deterministic-v1', '{}', TRUE, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ config_version_id, base.TEST_TENANT_ID, agent_id, profile_json });
    _ = try conn.exec(
        \\INSERT INTO workspace_active_config (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
        \\VALUES ($1, $2, $3, 'test', 0)
        \\ON CONFLICT DO NOTHING
    , .{ workspace_id, base.TEST_TENANT_ID, config_version_id });
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
        \\VALUES ($1, $2, $3, $4, $5, $5, 'deterministic-v1', '{}', TRUE, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ config_version_id, base.TEST_TENANT_ID, agent_id, version, profile_json });
}

pub fn seedRunWithSpec(
    conn: *pg.Conn,
    spec_id: []const u8,
    run_id: []const u8,
    workspace_id: []const u8,
) !void {
    try base.seedSpec(conn, spec_id, workspace_id);
    try base.seedRun(conn, run_id, workspace_id, spec_id);
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
        \\ON CONFLICT DO NOTHING
    , .{ run_id, run_id, agent_id, workspace_id, score, scored_at });
}

pub fn insertScoreWithRun(
    conn: *pg.Conn,
    spec_id: []const u8,
    run_id: []const u8,
    agent_id: []const u8,
    workspace_id: []const u8,
    score: i32,
    scored_at: i64,
) !void {
    try seedRunWithSpec(conn, spec_id, run_id, workspace_id);
    try insertScoreRow(conn, run_id, agent_id, workspace_id, score, scored_at);
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
