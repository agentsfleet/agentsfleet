// Section 7.14 — HTTP-path guard acceptance tests.
// Verifies that applyProposal (the backend guard beneath the HTTP handlers)
// rejects apply calls when no matching proposal record exists in APPROVED or
// VETO_WINDOW status, and succeeds when a valid VETO_WINDOW proposal is present.
const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const auto_approval = @import("scoring_mod/proposals_auto_approval.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

// A valid stage_insert change JSON matching the 3-stage default profile.
const STAGE_INSERT_CHANGE =
    \\[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_guard_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"quality"}]
;

test "applyProposal rejects when no proposal record exists for the given proposal_id" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_guard_1", "ws_guard_1", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_guard_1", "ws_guard_1", "0195b4ba-8d3a-7f13-8abc-3a0000000001");

    // No proposal row inserted — guard must reject.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_guard_1",
        "ws_guard_1",
        "non-existent-proposal-id",
        "0195b4ba-8d3a-7f13-8abc-3a0000000001",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        10_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = 'agent_guard_1'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 0), log_count);
}

test "applyProposal rejects when proposal exists but status is PENDING_REVIEW not VETO_WINDOW" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_guard_2", "ws_guard_2", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_guard_2", "ws_guard_2", "0195b4ba-8d3a-7f13-8abc-3a0000000003");

    // Insert a MANUAL/PENDING_REVIEW proposal — wrong status for auto-apply guard.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000004', 'agent_guard_2', 'ws_guard_2', 'DECLINING_SCORE', $1, '0195b4ba-8d3a-7f13-8abc-3a0000000003', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{STAGE_INSERT_CHANGE});

    // Guard called with required_status=VETO_WINDOW — mismatch → rejected.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_guard_2",
        "ws_guard_2",
        "0195b4ba-8d3a-7f13-8abc-3a0000000004",
        "0195b4ba-8d3a-7f13-8abc-3a0000000003",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        10_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = 'agent_guard_2'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 0), log_count);
}

test "applyProposal succeeds with a valid VETO_WINDOW proposal" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_guard_3', 'ws_guard_3', 'SCALE', 10, 20, 10, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_guard_3", "ws_guard_3", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_guard_3", "ws_guard_3", "0195b4ba-8d3a-7f13-8abc-3a0000000005");

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000006', 'agent_guard_3', 'ws_guard_3', 'DECLINING_SCORE', $1, '0195b4ba-8d3a-7f13-8abc-3a0000000005', 'AUTO', 'READY', 'VETO_WINDOW', 10_000, 100, 101)
    , .{STAGE_INSERT_CHANGE});

    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        "agent_guard_3",
        "ws_guard_3",
        "0195b4ba-8d3a-7f13-8abc-3a0000000006",
        "0195b4ba-8d3a-7f13-8abc-3a0000000005",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.applied, result);

    // harness_change_log must have exactly one row for this proposal.
    var log_q = try db_ctx.conn.query(
        \\SELECT field_name, applied_by
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-3a0000000006'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("stage_insert", log_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("system:auto", log_row.get([]const u8, 1) catch "");
    try std.testing.expect((try log_q.next()) == null);

    // workspace_active_config must have been updated to a new config_version_id.
    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_guard_3'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    const new_version_id = active_row.get([]const u8, 0) catch "";
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-3a0000000005", new_version_id));
    try std.testing.expect((try active_q.next()) == null);
}
