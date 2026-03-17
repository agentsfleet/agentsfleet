const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const proposals_auto_approval = @import("scoring_mod/proposals_auto_approval.zig");
const proposals_shared = @import("scoring_mod/proposals_shared.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

test "vetoAutoProposal stores veto reason and preserves harness state" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_veto_1', 'ws_prop_veto_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_prop_veto_1", "ws_prop_veto_1", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_veto_1", "ws_prop_veto_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb5");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb6', 'agent_prop_veto_1', 'ws_prop_veto_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_veto_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb5', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
    , .{});

    try std.testing.expect(try proposals.vetoAutoProposal(
        db_ctx.conn,
        "agent_prop_veto_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb6",
        "OPERATOR_VETOED",
        3_000,
    ));

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb6'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("VETOED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("OPERATOR_VETOED", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_veto_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb5", active_row.get([]const u8, 0) catch "");
    try std.testing.expect((try active_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT change_id
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb6'
    , .{});
    defer change_q.deinit();
    try std.testing.expect((try change_q.next()) == null);
}

test "approveManualProposal applies proposal with operator identity" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_2', 'ws_prop_manual_2', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_2", "ws_prop_manual_2");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_2", "ws_prop_manual_2", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc1");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc2', 'agent_prop_manual_2', 'ws_prop_manual_2', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_manual_2","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc1', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    const result = (try proposals.approveManualProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_prop_manual_2",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc2",
        "kishore",
        2_000,
    )) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(proposals.ApplyProposalResult.applied, result);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, applied_by
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc2'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("APPLIED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("operator:kishore", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_manual_2'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc1", active_row.get([]const u8, 0) catch ""));
    try std.testing.expect((try active_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT field_name, old_value, new_value, applied_by
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc2'
    , .{});
    defer change_q.deinit();
    const change_row = (try change_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("stage_insert", change_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("null", change_row.get([]const u8, 1) catch "");
    try std.testing.expect(std.mem.containsAtLeast(u8, change_row.get([]const u8, 2) catch "", 1, "\"insert_before_stage_id\":\"verify\""));
    try std.testing.expectEqualStrings("operator:kishore", change_row.get([]const u8, 3) catch "");
    try std.testing.expect((try change_q.next()) == null);

    var telemetry = (try proposals.loadAppliedProposalTelemetry(
        db_ctx.conn,
        std.testing.allocator,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fc2",
    )) orelse return error.TestUnexpectedResult;
    defer telemetry.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("agent_prop_manual_2", telemetry.agent_id);
    try std.testing.expectEqualStrings("ws_prop_manual_2", telemetry.workspace_id);
    try std.testing.expectEqualStrings("DECLINING_SCORE", telemetry.trigger_reason);
    try std.testing.expectEqualStrings("MANUAL", telemetry.approval_mode);
    try std.testing.expectEqual(@as(usize, 1), telemetry.fields_changed.len);
    try std.testing.expectEqualStrings("stage_insert", telemetry.fields_changed[0]);
}

test "approveManualProposal returns null when proposal was already applied" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_dup_1', 'ws_prop_manual_dup_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_dup_1", "ws_prop_manual_dup_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_dup_1", "ws_prop_manual_dup_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7204");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7205', 'agent_prop_manual_dup_1', 'ws_prop_manual_dup_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7204', 'MANUAL', 'READY', 'APPLIED', 'operator:kishore', NULL, 100, 101)
    , .{});

    try std.testing.expect((try proposals.approveManualProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_prop_manual_dup_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7205",
        "kishore",
        2_000,
    )) == null);
}

test "approveManualProposal rejects malformed proposal changes with compile failure" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_compile_1', 'ws_prop_manual_compile_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_compile_1", "ws_prop_manual_compile_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_compile_1", "ws_prop_manual_compile_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd3");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd4', 'agent_prop_manual_compile_1', 'ws_prop_manual_compile_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_manual_compile_1","insert_before_stage_id":"missing-stage","stage_id":"verify-precheck","role":"autoworkerready","skill":"warden","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"broken stage ref"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd3', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    const result = (try proposals.approveManualProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_prop_manual_compile_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd4",
        "kishore",
        4_000,
    )) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(proposals.ApplyProposalResult.rejected, result);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd4'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("COMPILE_FAILED", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT change_id
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd4'
    , .{});
    defer change_q.deinit();
    try std.testing.expect((try change_q.next()) == null);
}

test "approveManualProposal rejects activation failure when config context is missing" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_activate_1', 'ws_prop_manual_activate_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_activate_1", "ws_prop_manual_activate_1");
    try support.insertConfigVersionOnly(
        db_ctx.conn,
        "different_agent",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd5",
        1,
        "{\"agent_id\":\"different_agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"implement\",\"role\":\"scout\",\"skill\":\"scout\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}",
    );
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_active_config (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
        \\VALUES ('ws_prop_manual_activate_1', 'tenant_test', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd5', 'test', 0)
    , .{});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd6', 'agent_prop_manual_activate_1', 'ws_prop_manual_activate_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_manual_activate_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"warden","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd5', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    const result = (try proposals.approveManualProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_prop_manual_activate_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd6",
        "kishore",
        5_000,
    )) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(proposals.ApplyProposalResult.rejected, result);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd6'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("ACTIVATE_FAILED", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_manual_activate_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd5", active_row.get([]const u8, 0) catch "");
    try std.testing.expect((try active_q.next()) == null);
}

test "applyProposal requires proposal to enter approved state before harness mutation" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_guard_1', 'ws_prop_guard_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_guard_1", "ws_prop_guard_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_guard_1", "ws_prop_guard_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd7");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd8', 'agent_prop_guard_1', 'ws_prop_guard_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_prop_guard_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd7', 'MANUAL', 'READY', 'REJECTED', NULL, 100, 101)
    , .{});

    const result = try proposals_auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_prop_guard_1",
        "ws_prop_guard_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd8",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd7",
        "[{\"target_field\":\"stage_insert\",\"current_value\":null,\"proposed_value\":{\"agent_id\":\"agent_prop_guard_1\",\"insert_before_stage_id\":\"verify\",\"stage_id\":\"verify-precheck\",\"role\":\"autoworkerready\",\"skill\":\"clawhub://usezombie/autoworkerready@1.0.0\",\"artifact_name\":\"verify-precheck.md\",\"commit_message\":\"agent: add verify-precheck.md\",\"gate\":false,\"on_pass\":\"verify\",\"on_fail\":\"retry\"},\"rationale\":\"recover quality\"}]",
        proposals_shared.STATUS_PENDING_REVIEW,
        "operator:kishore",
        6_000,
    );
    try std.testing.expectEqual(proposals.ApplyProposalResult.rejected, result);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd8'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_prop_guard_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd7", active_row.get([]const u8, 0) catch "");
    try std.testing.expect((try active_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT change_id
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd8'
    , .{});
    defer change_q.deinit();
    try std.testing.expect((try change_q.next()) == null);
}

test "rejectManualProposal stores rejection reason" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_3', 'ws_prop_manual_3', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_3", "ws_prop_manual_3");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_3", "ws_prop_manual_3", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd1");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd2', 'agent_prop_manual_3', 'ws_prop_manual_3', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd1', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    try std.testing.expect(try proposals.rejectManualProposal(
        db_ctx.conn,
        "agent_prop_manual_3",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd2",
        "OPERATOR_REJECTED",
        3_000,
    ));

    var q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fd2'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("OPERATOR_REJECTED", row.get([]const u8, 1) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "vetoAutoProposal returns false for manual proposal" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_veto_manual_1', 'ws_prop_veto_manual_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_veto_manual_1", "ws_prop_veto_manual_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_veto_manual_1", "ws_prop_veto_manual_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7206");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7207', 'agent_prop_veto_manual_1', 'ws_prop_veto_manual_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7206', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    try std.testing.expect(!try proposals.vetoAutoProposal(
        db_ctx.conn,
        "agent_prop_veto_manual_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7207",
        "OPERATOR_VETOED",
        3_000,
    ));
}

test "reconcileDueAutoApprovalProposals expires manual proposal at exact cutoff" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_expiry_exact_1', 'ws_prop_expiry_exact_1', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_expiry_exact_1", "ws_prop_expiry_exact_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_expiry_exact_1", "ws_prop_expiry_exact_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7208");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7209', 'agent_prop_expiry_exact_1', 'ws_prop_expiry_exact_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7208', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    const now_ms = 100 + proposals_shared.MANUAL_PROPOSAL_EXPIRY_MS;
    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, now_ms);
    try std.testing.expectEqual(@as(u32, 1), result.expired);

    var q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7209'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("EXPIRED", row.get([]const u8, 1) catch "");
}

test "reconcileDueAutoApprovalProposals expires stale manual proposals" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_prop_manual_4', 'ws_prop_manual_4', 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfile(db_ctx.conn, "agent_prop_manual_4", "ws_prop_manual_4");
    try support.insertActiveConfig(db_ctx.conn, "agent_prop_manual_4", "ws_prop_manual_4", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fe1");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fe2', 'agent_prop_manual_4', 'ws_prop_manual_4', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fe1', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 0, 0)
    , .{});

    const now_ms = proposals_shared.MANUAL_PROPOSAL_EXPIRY_MS + 1;
    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, now_ms);
    try std.testing.expectEqual(@as(u32, 1), result.expired);

    var q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fe2'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("EXPIRED", row.get([]const u8, 1) catch "");
    try std.testing.expect((try q.next()) == null);
}
