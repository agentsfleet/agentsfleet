const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");
const base = @import("../db/test_fixtures.zig");
const uc2 = @import("../db/test_fixtures_uc2.zig");

const WS_AUTO_1 = "0195b4ba-8d3a-7f13-8abc-cc0000000112";
const WS_AUTO_2 = "0195b4ba-8d3a-7f13-8abc-cc0000000113";
const WS_AUTO_EQ = "0195b4ba-8d3a-7f13-8abc-cc0000000114";
const WS_MANUAL_1 = "0195b4ba-8d3a-7f13-8abc-cc0000000117";

test "reconcileDueAutoApprovalProposals applies overdue veto-window proposals" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_1);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_AUTO_1);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_1);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000112', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_AUTO_1});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_AUTO_1, WS_AUTO_1, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_AUTO_1, WS_AUTO_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa2");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const spec_id = try support.allocTestUuid(std.testing.allocator, 0x111200000000 + i);
        defer std.testing.allocator.free(spec_id);
        const run_id = try support.allocTestUuid(std.testing.allocator, 0x111210000000 + i);
        defer std.testing.allocator.free(run_id);
        try support.insertScoreWithRun(db_ctx.conn, spec_id, run_id, uc2.AGENT_PROP_AUTO_1, WS_AUTO_1, 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const spec_id = try support.allocTestUuid(std.testing.allocator, 0x111220000000 + i);
        defer std.testing.allocator.free(spec_id);
        const run_id = try support.allocTestUuid(std.testing.allocator, 0x111230000000 + i);
        defer std.testing.allocator.free(run_id);
        try support.insertScoreWithRun(db_ctx.conn, spec_id, run_id, uc2.AGENT_PROP_AUTO_1, WS_AUTO_1, 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, WS_AUTO_1, uc2.AGENT_PROP_AUTO_1, 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    const now_ms: i64 = 20_000;
    _ = try db_ctx.conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1::uuid
    , .{ uc2.AGENT_PROP_AUTO_1, now_ms });

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, now_ms + 1);
    try std.testing.expectEqual(@as(u32, 1), result.applied);
    try std.testing.expectEqual(@as(u32, 0), result.config_changed);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, applied_by
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_AUTO_1});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("APPLIED", proposal_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("system:auto", proposal_row.get([]const u8, 1) catch "");
    try std.testing.expect((try proposal_q.next()) == null);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id::text
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
    , .{WS_AUTO_1});
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
        \\  WHERE agent_id = $1
        \\)
    , .{uc2.AGENT_PROP_AUTO_1});
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

    uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_EQ);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_AUTO_EQ);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_EQ);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000114', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_AUTO_EQ});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_AUTO_EQ_1, WS_AUTO_EQ, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_AUTO_EQ_1, WS_AUTO_EQ, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7202");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7203', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd000000001a","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7202', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_AUTO_EQ_1, WS_AUTO_EQ });

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, 20_000);
    try std.testing.expectEqual(@as(u32, 1), result.applied);
}

test "reconcileDueAutoApprovalProposals rejects auto-apply when config version changed" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_2);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_AUTO_2);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_AUTO_2);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000113', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_AUTO_2});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_PROP_AUTO_2, WS_AUTO_2, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_AUTO_2, WS_AUTO_2, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa3");

    var i: usize = 0;
    while (i < 5) : (i += 1) {
        const spec_id = try support.allocTestUuid(std.testing.allocator, 0x111240000000 + i);
        defer std.testing.allocator.free(spec_id);
        const run_id = try support.allocTestUuid(std.testing.allocator, 0x111250000000 + i);
        defer std.testing.allocator.free(run_id);
        try support.insertScoreWithRun(db_ctx.conn, spec_id, run_id, uc2.AGENT_PROP_AUTO_2, WS_AUTO_2, 95, @as(i64, @intCast(i + 1)));
    }
    while (i < 10) : (i += 1) {
        const spec_id = try support.allocTestUuid(std.testing.allocator, 0x111260000000 + i);
        defer std.testing.allocator.free(spec_id);
        const run_id = try support.allocTestUuid(std.testing.allocator, 0x111270000000 + i);
        defer std.testing.allocator.free(run_id);
        try support.insertScoreWithRun(db_ctx.conn, spec_id, run_id, uc2.AGENT_PROP_AUTO_2, WS_AUTO_2, 80, @as(i64, @intCast(i + 1)));
    }

    try proposals.maybePersistTriggerProposal(db_ctx.conn, std.testing.allocator, WS_AUTO_2, uc2.AGENT_PROP_AUTO_2, 11_000);
    const generated = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 1), generated.ready);

    try support.insertConfigVersionOnly(db_ctx.conn, uc2.AGENT_PROP_AUTO_2, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4", 2, "{\"agent_id\":\"agent\",\"stages\":[{\"stage_id\":\"plan\",\"role\":\"echo\",\"skill\":\"echo\"},{\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\",\"gate\":true,\"on_pass\":\"done\",\"on_fail\":\"retry\"}]}");
    _ = try db_ctx.conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fa4'
        \\WHERE workspace_id = $1::uuid
    , .{WS_AUTO_2});

    const due_ms: i64 = 20_000;
    _ = try db_ctx.conn.exec(
        \\UPDATE agent_improvement_proposals
        \\SET auto_apply_at = $2
        \\WHERE agent_id = $1::uuid
    , .{ uc2.AGENT_PROP_AUTO_2, due_ms });

    const result = try proposals.reconcileDueAutoApprovalProposals(db_ctx.conn, std.testing.allocator, 0, due_ms + 1);
    try std.testing.expectEqual(@as(u32, 0), result.applied);
    try std.testing.expectEqual(@as(u32, 1), result.config_changed);

    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = $1
    , .{uc2.AGENT_PROP_AUTO_2});
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

    uc2.teardownWorkspace(db_ctx.conn, WS_MANUAL_1);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_MANUAL_1);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_MANUAL_1);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000117', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_MANUAL_1});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_MANUAL_1, WS_MANUAL_1);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_MANUAL_1, WS_MANUAL_1, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb4', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd000000001d","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'AUTO', 'READY', 'VETO_WINDOW', 250, 90, 90),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb2', $1, $2, 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd000000001d","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"recover quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb3', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6fb1', 'MANUAL', 'READY', 'REJECTED', NULL, 99, 99)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_MANUAL_1, WS_MANUAL_1 });

    const items = try proposals.listOpenProposals(db_ctx.conn, std.testing.allocator, uc2.AGENT_PROP_MANUAL_1, 0);
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
