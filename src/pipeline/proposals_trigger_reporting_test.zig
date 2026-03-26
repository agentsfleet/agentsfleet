const std = @import("std");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");
const proposals_shared = @import("scoring_mod/proposals_shared.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");
const base = @import("../db/test_fixtures.zig");
const uc2 = @import("../db/test_fixtures_uc2.zig");

const WS_PROP_1 = "0195b4ba-8d3a-7f13-8abc-cc0000000101";
const WS_PROP_2 = "0195b4ba-8d3a-7f13-8abc-cc0000000102";
const WS_PROP_3 = "0195b4ba-8d3a-7f13-8abc-cc0000000103";
const WS_PROP_4 = "0195b4ba-8d3a-7f13-8abc-cc0000000104";
const WS_PROP_5 = "0195b4ba-8d3a-7f13-8abc-cc0000000108";
const WS_PROP_TRUSTED = "0195b4ba-8d3a-7f13-8abc-cc0000000106";
const WS_PROP_THRESHOLD = "0195b4ba-8d3a-7f13-8abc-cc0000000105";
const WS_PROP_REPORT = "0195b4ba-8d3a-7f13-8abc-cc0000000107";

fn seedRunFixture(conn: anytype, seed: u64, workspace_id: []const u8) ![]u8 {
    const spec_id = try support.allocTestUuid(std.testing.allocator, 0x121100000000 + seed);
    defer std.testing.allocator.free(spec_id);
    const run_id = try support.allocTestUuid(std.testing.allocator, 0x121200000000 + seed);
    errdefer std.testing.allocator.free(run_id);
    try support.seedRunWithSpec(conn, spec_id, run_id, workspace_id);
    return run_id;
}

fn insertScoreFixture(conn: anytype, seed: u64, agent_id: []const u8, workspace_id: []const u8, score: i32, scored_at: i64) !void {
    // check-pg-drain: ok — helper uses support.insertScoreWithRun, which only calls exec-backed fixture helpers
    const spec_id = try support.allocTestUuid(std.testing.allocator, 0x121300000000 + seed);
    defer std.testing.allocator.free(spec_id);
    const run_id = try support.allocTestUuid(std.testing.allocator, 0x121400000000 + seed);
    defer std.testing.allocator.free(run_id);
    try support.insertScoreWithRun(conn, spec_id, run_id, agent_id, workspace_id, score, scored_at);
}

test "scoreRunIfTerminal persists proposal groundwork after sustained low-score window" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_1);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_1);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_1);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000101', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_1});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_1, WS_PROP_1);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_1, WS_PROP_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try seedRunFixture(db_ctx.conn, 0x0100 + i, WS_PROP_1);
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, WS_PROP_1, uc2.AGENT_PROP_1, "user_prop_1", &low_state, 20);
    }

    var q = try db_ctx.conn.query(
        \\SELECT trigger_reason, proposed_changes, approval_mode, generation_status, status, config_version_id::text, auto_apply_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_1});
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
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_2);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_2);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_2);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000102', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_2});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_latency_baseline
        \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\VALUES ($1, 10, 30, 5, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_2});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_2, WS_PROP_2);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_2, WS_PROP_2, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93");

    const high_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try seedRunFixture(db_ctx.conn, 0x0200 + i, WS_PROP_2);
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, WS_PROP_2, uc2.AGENT_PROP_2, "user_prop_2", &high_state, 8);
    }

    const medium_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 1, .stages_total = 2 };
    i = 0;
    while (i < 5) : (i += 1) {
        const run_id = try seedRunFixture(db_ctx.conn, 0x0300 + i, WS_PROP_2);
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, WS_PROP_2, uc2.AGENT_PROP_2, "user_prop_2", &medium_state, 20);
    }

    var q = try db_ctx.conn.query(
        \\SELECT trigger_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_2});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("DECLINING_SCORE", row.get([]const u8, 0) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "scoreRunIfTerminal does not trigger at exact sustained-low threshold average of 60" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_THRESHOLD);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_THRESHOLD);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_THRESHOLD);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000105', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_THRESHOLD});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_THRESHOLD_1, WS_PROP_THRESHOLD);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_THRESHOLD_1, WS_PROP_THRESHOLD, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7201");

    var idx: usize = 0;
    while (idx < 5) : (idx += 1) {
        try insertScoreFixture(db_ctx.conn, 0x0400 + idx, uc2.AGENT_PROP_THRESHOLD_1, WS_PROP_THRESHOLD, 60, @as(i64, @intCast(idx + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, WS_PROP_THRESHOLD, uc2.AGENT_PROP_THRESHOLD_1, 6_000);

    var q = try db_ctx.conn.query(
        \\SELECT proposal_id
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_THRESHOLD_1});
    defer q.deinit();
    try std.testing.expect((try q.next()) == null);
}

test "loadImprovementReport summarizes counts, tiers, and stalled warning" {
    const report_profile_json = "{\"agent_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"implement\",\"role\":\"scout\",\"skill\":\"scout\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}";
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_REPORT);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_REPORT);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_REPORT);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_REPORT_1, WS_PROP_REPORT, 0, "UNEARNED");
    try support.insertActiveConfigWithProfile(db_ctx.conn, uc2.AGENT_REPORT_1, WS_PROP_REPORT, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad1", report_profile_json);
    try support.insertConfigVersionOnly(db_ctx.conn, uc2.AGENT_REPORT_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad2", 2, report_profile_json);
    try support.insertConfigVersionOnly(db_ctx.conn, uc2.AGENT_REPORT_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad3", 3, report_profile_json);
    try support.insertConfigVersionOnly(db_ctx.conn, uc2.AGENT_REPORT_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad4", 4, report_profile_json);
    try support.insertConfigVersionOnly(db_ctx.conn, uc2.AGENT_REPORT_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad5", 5, report_profile_json);

    var idx: usize = 0;
    while (idx < 5) : (idx += 1) {
        try insertScoreFixture(db_ctx.conn, 0x0500 + idx, uc2.AGENT_REPORT_1, WS_PROP_REPORT, 90, @intCast(idx + 1));
    }
    idx = 0;
    while (idx < 5) : (idx += 1) {
        try insertScoreFixture(db_ctx.conn, 0x0600 + idx, uc2.AGENT_REPORT_1, WS_PROP_REPORT, 30, @intCast(200 + idx));
    }

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac1', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad1', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 100, 100),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac2', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad2', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 110, 110),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac3', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad3', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 120, 120),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac4', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad4', 'AUTO', 'READY', 'VETOED', NULL, 130, 130),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac5', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ad5', 'MANUAL', 'READY', 'REJECTED', NULL, 140, 140)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_REPORT_1, WS_PROP_REPORT });
    _ = try db_ctx.conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from, score_delta)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ae1', $1, '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac1', $2, 'stage_insert', '{}', '{}', 100, 'operator:test', NULL, -5.0),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ae2', $1, '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac2', $2, 'stage_insert', '{}', '{}', 110, 'operator:test', NULL, -10.0),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ae3', $1, '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac3', $2, 'stage_insert', '{}', '{}', 120, 'operator:test', NULL, -15.0)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_REPORT_1, WS_PROP_REPORT });

    var report = (try proposals.loadImprovementReport(db_ctx.conn, std.testing.allocator, uc2.AGENT_REPORT_1)) orelse return error.TestUnexpectedResult;
    defer report.deinit(std.testing.allocator);

    try std.testing.expectEqualStrings(uc2.AGENT_REPORT_1, report.agent_id);
    try std.testing.expectEqualStrings("UNEARNED", report.trust_level);
    try std.testing.expect(report.improvement_stalled_warning);
    try std.testing.expectEqual(@as(u32, 5), report.proposals_generated);
    try std.testing.expectEqual(@as(u32, 3), report.proposals_approved);
    try std.testing.expectEqual(@as(u32, 1), report.proposals_vetoed);
    try std.testing.expectEqual(@as(u32, 1), report.proposals_rejected);
    try std.testing.expectEqual(@as(u32, 3), report.proposals_applied);
    try std.testing.expect(report.avg_score_delta_per_applied_change != null);
    try std.testing.expect(std.math.approxEqAbs(f64, -10.0, report.avg_score_delta_per_applied_change.?, 0.001));
    try std.testing.expectEqualStrings("Bronze", report.current_tier.?);
    try std.testing.expectEqualStrings("Elite", report.baseline_tier.?);
}

test "trusted proposal enters veto window with auto-apply deadline" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_TRUSTED);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_TRUSTED);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_TRUSTED);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000106', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_TRUSTED});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_TRUSTED_1, WS_PROP_TRUSTED, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_TRUSTED_1, WS_PROP_TRUSTED, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa1");

    var ts: i64 = 1_000;
    while (ts < 6_000) : (ts += 1_000) {
        try insertScoreFixture(db_ctx.conn, 0x0700 + @as(u64, @intCast(@divTrunc(ts, 1_000))), uc2.AGENT_PROP_TRUSTED_1, WS_PROP_TRUSTED, 95, ts);
    }
    while (ts < 11_000) : (ts += 1_000) {
        try insertScoreFixture(db_ctx.conn, 0x0800 + @as(u64, @intCast(@divTrunc(ts, 1_000))), uc2.AGENT_PROP_TRUSTED_1, WS_PROP_TRUSTED, 80, ts);
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, WS_PROP_TRUSTED, uc2.AGENT_PROP_TRUSTED_1, 11_000);

    var q = try db_ctx.conn.query(
        \\SELECT approval_mode, status, auto_apply_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_TRUSTED_1});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("AUTO", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("VETO_WINDOW", row.get([]const u8, 1) catch "");
    try std.testing.expectEqual(@as(i64, 11_000 + proposals_shared.AUTO_APPLY_WINDOW_MS), row.get(?i64, 2) catch null orelse -1);
    try std.testing.expect((try q.next()) == null);
}

test "reconcilePendingProposalGenerations materializes generated stage proposal payload" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_4);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_4);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_4);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000104', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_4});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_4, WS_PROP_4);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_4, WS_PROP_4, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try seedRunFixture(db_ctx.conn, 0x0900 + i, WS_PROP_4);
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, WS_PROP_4, uc2.AGENT_PROP_4, "user_prop_4", &low_state, 20);
    }

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), result.ready);
    try std.testing.expectEqual(@as(u32, 0), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes, generation_status, status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_4});
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

test "proposal validation rejects unregistered agent refs and entitlement-disallowed skills" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_3);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_3);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_3);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000103', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_3});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_3, WS_PROP_3);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_3, WS_PROP_3, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f96");

    try std.testing.expectError(
        proposals.ProposalValidationError.UnregisteredAgentRef,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            WS_PROP_3,
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"missing-agent\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\"},\"rationale\":\"rebind stage\"}]",
        ),
    );

    try std.testing.expectError(
        proposals.ProposalValidationError.EntitlementSkillNotAllowed,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            WS_PROP_3,
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"" ++ uc2.AGENT_PROP_3 ++ "\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"clawhub://openclaw/github-reviewer@1.2.0\"},\"rationale\":\"rebind stage\"}]",
        ),
    );
}

test "reconcilePendingProposalGenerations rejects generated proposals that exceed stage entitlements" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_PROP_5);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_PROP_5);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_PROP_5);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000108', $1, 'FREE', 3, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_PROP_5});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_5, WS_PROP_5);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_5, WS_PROP_5, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99', $1, $2, 'SUSTAINED_LOW_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95', 'MANUAL', 'PENDING', 'PENDING_REVIEW', NULL, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_5, WS_PROP_5 });

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 0), result.ready);
    try std.testing.expectEqual(@as(u32, 1), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes, generation_status, status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("[]", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 2) catch "");
    try std.testing.expectEqualStrings("UZ-ENTL-003", row.get([]const u8, 3) catch "");
    try std.testing.expect((try q.next()) == null);
}
