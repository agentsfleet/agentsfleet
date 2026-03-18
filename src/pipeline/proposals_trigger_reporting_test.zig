const std = @import("std");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");
const proposals_shared = @import("scoring_mod/proposals_shared.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

test "scoreRunIfTerminal persists proposal groundwork after sustained low-score window" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000101', '0195b4ba-8d3a-7f13-8abc-cc0000000101', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_1", "0195b4ba-8d3a-7f13-8abc-cc0000000101");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_1", "0195b4ba-8d3a-7f13-8abc-cc0000000101", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f92");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_low_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "0195b4ba-8d3a-7f13-8abc-cc0000000101", "agent_prop_1", "user_prop_1", &low_state, 20);
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
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000102', '0195b4ba-8d3a-7f13-8abc-cc0000000102', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_latency_baseline
        \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-cc0000000102', 10, 30, 5, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_2", "0195b4ba-8d3a-7f13-8abc-cc0000000102");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_2", "0195b4ba-8d3a-7f13-8abc-cc0000000102", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f93");

    const high_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 2, .stages_total = 2 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_high_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "0195b4ba-8d3a-7f13-8abc-cc0000000102", "agent_prop_2", "user_prop_2", &high_state, 8);
    }

    const medium_state = scoring.ScoringState{ .outcome = .done, .stages_passed = 1, .stages_total = 2 };
    i = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_med_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "0195b4ba-8d3a-7f13-8abc-cc0000000102", "agent_prop_2", "user_prop_2", &medium_state, 20);
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

test "scoreRunIfTerminal does not trigger at exact sustained-low threshold average of 60" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000105', '0195b4ba-8d3a-7f13-8abc-cc0000000105', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_threshold_1", "0195b4ba-8d3a-7f13-8abc-cc0000000105");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_threshold_1", "0195b4ba-8d3a-7f13-8abc-cc0000000105", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7201");

    var idx: usize = 0;
    while (idx < 5) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_threshold_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_threshold_1", "0195b4ba-8d3a-7f13-8abc-cc0000000105", 60, @as(i64, @intCast(idx + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-cc0000000105", "agent_prop_threshold_1", 6_000);

    var q = try db_ctx.conn.query(
        \\SELECT proposal_id
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_prop_threshold_1'
    , .{});
    defer q.deinit();
    try std.testing.expect((try q.next()) == null);
}

test "loadImprovementReport summarizes counts, tiers, and stalled warning" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_report_1", "0195b4ba-8d3a-7f13-8abc-cc0000000107", 0, "UNEARNED");

    var idx: usize = 0;
    while (idx < 5) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "report_hist_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_report_1", "0195b4ba-8d3a-7f13-8abc-cc0000000107", 90, @intCast(idx + 1));
    }
    idx = 0;
    while (idx < 5) : (idx += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "report_current_{d}", .{idx});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_report_1", "0195b4ba-8d3a-7f13-8abc-cc0000000107", 30, @intCast(200 + idx));
    }

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac1', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'DECLINING_SCORE', '[]', 'cfg_1', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 100, 100),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac2', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'DECLINING_SCORE', '[]', 'cfg_2', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 110, 110),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac3', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'DECLINING_SCORE', '[]', 'cfg_3', 'MANUAL', 'READY', 'APPLIED', 'operator:test', 120, 120),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac4', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'DECLINING_SCORE', '[]', 'cfg_4', 'AUTO', 'READY', 'VETOED', NULL, 130, 130),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac5', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'DECLINING_SCORE', '[]', 'cfg_5', 'MANUAL', 'READY', 'REJECTED', NULL, 140, 140)
    , .{});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from, score_delta)
        \\VALUES
        \\  ('chg_report_1', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac1', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'stage_insert', '{}', '{}', 100, 'operator:test', NULL, -5.0),
        \\  ('chg_report_2', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac2', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'stage_insert', '{}', '{}', 110, 'operator:test', NULL, -10.0),
        \\  ('chg_report_3', 'agent_report_1', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ac3', '0195b4ba-8d3a-7f13-8abc-cc0000000107', 'stage_insert', '{}', '{}', 120, 'operator:test', NULL, -15.0)
    , .{});

    var report = (try proposals.loadImprovementReport(db_ctx.conn, std.testing.allocator, "agent_report_1")) orelse return error.TestUnexpectedResult;
    defer report.deinit(std.testing.allocator);

    try std.testing.expectEqualStrings("agent_report_1", report.agent_id);
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

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000106', '0195b4ba-8d3a-7f13-8abc-cc0000000106', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_trusted_1", "0195b4ba-8d3a-7f13-8abc-cc0000000106", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_trusted_1", "0195b4ba-8d3a-7f13-8abc-cc0000000106", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa1");

    var ts: i64 = 1_000;
    while (ts < 6_000) : (ts += 1_000) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prev_{d}", .{ts});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_trusted_1", "0195b4ba-8d3a-7f13-8abc-cc0000000106", 95, ts);
    }
    while (ts < 11_000) : (ts += 1_000) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_curr_{d}", .{ts});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_trusted_1", "0195b4ba-8d3a-7f13-8abc-cc0000000106", 80, ts);
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-cc0000000106", "agent_prop_trusted_1", 11_000);

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
    try std.testing.expect((try q.next()) == null);
}

test "reconcilePendingProposalGenerations materializes generated stage proposal payload" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000104', '0195b4ba-8d3a-7f13-8abc-cc0000000104', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_4", "0195b4ba-8d3a-7f13-8abc-cc0000000104");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_4", "0195b4ba-8d3a-7f13-8abc-cc0000000104", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f94");

    const low_state = scoring.ScoringState{ .outcome = .blocked_stage_graph, .stages_passed = 0, .stages_total = 3 };
    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_prop_ready_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        scoring.scoreRunIfTerminal(db_ctx.conn, null, run_id, "0195b4ba-8d3a-7f13-8abc-cc0000000104", "agent_prop_4", "user_prop_4", &low_state, 20);
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
