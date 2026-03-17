const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

test "reconcileDueAutoApprovalProposals applies overdue veto-window proposals" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_auto_1', 'ws_prop_auto_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_auto_1", "ws_prop_auto_1", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_auto_1", "ws_prop_auto_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_prev_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_1", "ws_prop_auto_1", 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_curr_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_1", "ws_prop_auto_1", 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "ws_prop_auto_1", "agent_prop_auto_1", 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    const now_ms: i64 = 20_000;
    _ = try db_ctx.conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1
    , .{ "agent_prop_auto_1", now_ms });

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
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_auto_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    const activated_config_version_id = active_row.get([]const u8, 0) catch "";
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2", activated_config_version_id));
    try std.testing.expect((try active_q.next()) == null);

    var log_q = try db_ctx.conn.query(
        \\SELECT field_name, old_value, new_value, applied_by
        \\FROM harness_change_log
        \\WHERE proposal_id = (
        \\  SELECT proposal_id
        \\  FROM agent_improvement_proposals
        \\  WHERE agent_id = 'agent_prop_auto_1'
        \\)
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("stage_insert", log_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("null", log_row.get([]const u8, 1) catch "");
    try std.testing.expect(std.mem.containsAtLeast(u8, log_row.get([]const u8, 2) catch "", 1, "\"stage_id\":\"verify-precheck\""));
    try std.testing.expectEqualStrings("system:auto", log_row.get([]const u8, 3) catch "");
    try std.testing.expect((try log_q.next()) == null);
}

test "reconcileDueAutoApprovalProposals applies proposal when auto_apply_at equals now" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_auto_eq_1', 'ws_prop_auto_eq_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_auto_eq_1", "ws_prop_auto_eq_1", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_auto_eq_1", "ws_prop_auto_eq_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7202");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7203', 'agent_prop_auto_eq_1', 'ws_prop_auto_eq_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_auto_eq_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7202', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
    , .{});

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, 20_000);
    try std.testing.expectEqual(@as(u32, 1), result.applied);
}

test "reconcileDueAutoApprovalProposals rejects auto-apply when config version changed" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_auto_2', 'ws_prop_auto_2', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_auto_2", "ws_prop_auto_2", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_auto_2", "ws_prop_auto_2", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa3");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_cfg_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_2", "ws_prop_auto_2", 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_auto_cfg_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_prop_auto_2", "ws_prop_auto_2", 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, "ws_prop_auto_2", "agent_prop_auto_2", 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    try support.insertConfigVersionOnly(db_ctx.conn, "agent_prop_auto_2", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4", 2, "{\"agent_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}");
    _ = try db_ctx.conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4'
        \\WHERE workspace_id = 'ws_prop_auto_2'
    , .{});

    const due_ms: i64 = 20_000;
    _ = try db_ctx.conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1
    , .{ "agent_prop_auto_2", due_ms });

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
    try std.testing.expect((try proposal_q.next()) == null);
}

test "listOpenProposals returns veto-window proposals before manual review proposals" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_1', 'ws_prop_manual_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_1", "ws_prop_manual_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_1", "ws_prop_manual_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb4', 'agent_prop_manual_1', 'ws_prop_manual_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_manual_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'AUTO', 'READY', 'VETO_WINDOW', 250, 90, 90),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb2', 'agent_prop_manual_1', 'ws_prop_manual_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_manual_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb3', 'agent_prop_manual_1', 'ws_prop_manual_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'MANUAL', 'READY', 'REJECTED', NULL, 99, 99)
    , .{});

    const items = try proposals.listOpenProposals(db_ctx.conn, std.testing.allocator, "agent_prop_manual_1", 0);
    defer {
        for (items) |*item| item.deinit(std.testing.allocator);
        std.testing.allocator.free(items);
    }

    try std.testing.expectEqual(@as(usize, 2), items.len);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb4", items[0].proposal_id);
    try std.testing.expectEqualStrings("VETO_WINDOW", items[0].status);
    try std.testing.expectEqualStrings("AUTO", items[0].approval_mode);
    try std.testing.expectEqual(@as(?i64, 250), items[0].auto_apply_at);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb2", items[1].proposal_id);
    try std.testing.expectEqualStrings("PENDING_REVIEW", items[1].status);
    try std.testing.expectEqualStrings("MANUAL", items[1].approval_mode);
    try std.testing.expect(std.mem.containsAtLeast(u8, items[0].proposed_changes, 1, "\"stage_insert\""));
}
