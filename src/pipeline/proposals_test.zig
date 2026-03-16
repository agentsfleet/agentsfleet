const std = @import("std");
const pg = @import("pg");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");

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
        \\  workspace_id TEXT NOT NULL,
        \\  trust_streak_runs INTEGER NOT NULL,
        \\  trust_level TEXT NOT NULL,
        \\  last_scored_at BIGINT,
        \\  updated_at BIGINT NOT NULL DEFAULT 0
        \\) ON COMMIT DROP
    );
    try execSql(conn,
        \\CREATE TEMP TABLE agent_config_versions (
        \\  config_version_id TEXT PRIMARY KEY,
        \\  agent_id TEXT NOT NULL,
        \\  compiled_profile_json TEXT
        \\) ON COMMIT DROP
    );
    try execSql(conn,
        \\CREATE TEMP TABLE workspace_active_config (
        \\  workspace_id TEXT PRIMARY KEY,
        \\  config_version_id TEXT NOT NULL
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
    var q = try conn.query(
        \\INSERT INTO agent_profiles (agent_id, workspace_id, trust_streak_runs, trust_level, last_scored_at, updated_at)
        \\VALUES ($1, $2, 0, 'UNEARNED', NULL, 0)
    , .{ agent_id, workspace_id });
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
            \\INSERT INTO agent_config_versions (config_version_id, agent_id, compiled_profile_json)
            \\VALUES ($1, $2, $3)
        , .{ config_version_id, agent_id, profile_json });
        q.deinit();
    }
    {
        var q = try conn.query(
            \\INSERT INTO workspace_active_config (workspace_id, config_version_id)
            \\VALUES ($1, $2)
        , .{ workspace_id, config_version_id });
        q.deinit();
    }
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
