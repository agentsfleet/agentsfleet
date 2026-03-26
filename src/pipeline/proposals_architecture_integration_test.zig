const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const proposals_shared = @import("scoring_mod/proposals_shared.zig");
const proposal_helpers = @import("proposals_test_support.zig");
const common = @import("../http/handlers/common.zig");
const base = @import("../db/test_fixtures.zig");
const uc2 = @import("../db/test_fixtures_uc2.zig");

const WS_ARCH_111 = "0195b4ba-8d3a-7f13-8abc-cc0000000111";
const WS_ARCH_110 = "0195b4ba-8d3a-7f13-8abc-cc0000000110";
const WS_ARCH_109 = "0195b4ba-8d3a-7f13-8abc-cc0000000109";

test "integration: trusted vetoed proposal stays canceled after reconcile passes deadline" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_111);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_ARCH_111);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_111);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000111', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_ARCH_111});
    try proposal_helpers.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_ARCH_VETO_1, WS_ARCH_111, 10, "TRUSTED");
    try proposal_helpers.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_ARCH_VETO_1, WS_ARCH_111, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7102', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd0000000017","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_ARCH_VETO_1, WS_ARCH_111 });

    try std.testing.expect(try proposals.vetoAutoProposal(
        db_ctx.conn,
        uc2.AGENT_PROP_ARCH_VETO_1,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7102",
        "OPERATOR_VETOED",
        3_000,
    ));

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, 25_000);
    try std.testing.expectEqual(@as(u32, 0), result.applied);
    try std.testing.expectEqual(@as(u32, 0), result.config_changed);
    try std.testing.expectEqual(@as(u32, 0), result.rejected);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7102'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("VETOED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("OPERATOR_VETOED", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id::text
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
    , .{WS_ARCH_111});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a7101", active_row.get([]const u8, 0) catch "");
    try std.testing.expect((try active_q.next()) == null);
}

test "integration: manual approval CAS drift rejects change and leaves active harness untouched" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_110);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_ARCH_110);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_110);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000110', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_ARCH_110});
    try proposal_helpers.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_ARCH_MANUAL_1, WS_ARCH_110);
    try proposal_helpers.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_ARCH_MANUAL_1, WS_ARCH_110, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7103");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7104', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd0000000016","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7103', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_ARCH_MANUAL_1, WS_ARCH_110 });
    try proposal_helpers.insertConfigVersionOnly(
        db_ctx.conn,
        uc2.AGENT_PROP_ARCH_MANUAL_1,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7105",
        2,
        "{\"agent_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}",
    );
    _ = try db_ctx.conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7105'
        \\WHERE workspace_id = $1
    , .{WS_ARCH_110});

    const outcome = (try proposals.approveManualProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_PROP_ARCH_MANUAL_1,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7104",
        "kishore",
        4_000,
    )) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(proposals.ApplyProposalResult.config_changed, outcome);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7104'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("CONFIG_CHANGED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings(proposals_shared.REJECTION_REASON_CONFIG_CHANGED_SINCE_PROPOSAL, proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id::text
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
    , .{WS_ARCH_110});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a7105", active_row.get([]const u8, 0) catch "");
    try std.testing.expect((try active_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT change_id
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7104'
    , .{});
    defer change_q.deinit();
    try std.testing.expect((try change_q.next()) == null);
}

test "integration: second reconcile pass is idempotent after first auto-apply" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_109);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_ARCH_109);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_ARCH_109);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000109', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_ARCH_109});
    try proposal_helpers.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_ARCH_IDEM_1, WS_ARCH_109, 10, "TRUSTED");
    try proposal_helpers.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_ARCH_IDEM_1, WS_ARCH_109, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7210");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7211', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd0000000015","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7210', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_ARCH_IDEM_1, WS_ARCH_109 });

    const first = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, 20_000);
    const second = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, 20_001);
    try std.testing.expectEqual(@as(u32, 1), first.applied);
    try std.testing.expectEqual(@as(u32, 0), second.applied);

    var change_q = try db_ctx.conn.query(
        \\SELECT COUNT(*)
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7211'
    , .{});
    defer change_q.deinit();
    const row = (try change_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(@as(i64, 1), row.get(i64, 0) catch -1);
}
