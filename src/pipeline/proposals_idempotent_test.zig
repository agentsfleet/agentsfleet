// Tests for repeated-action safety: duplicate reject/veto, reject-on-applied,
// and listOpenProposals filtering by generation_status.
const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

test "rejectManualProposal returns false on second call — no double mutation" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfile(db_ctx.conn, "agent_idem_rej_1", "ws_idem_rej_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_idem_rej_1", "ws_idem_rej_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0001");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0b0002', 'agent_idem_rej_1', 'ws_idem_rej_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0001', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
    , .{});

    const first = try proposals.rejectManualProposal(
        db_ctx.conn,
        "agent_idem_rej_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0002",
        "OPERATOR_REJECTED",
        1_000,
    );
    try std.testing.expect(first);

    const second = try proposals.rejectManualProposal(
        db_ctx.conn,
        "agent_idem_rej_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0002",
        "OPERATOR_REJECTED",
        2_000,
    );
    try std.testing.expect(!second);

    // Status must still be REJECTED with original reason, updated_at unchanged from first call.
    var q = try db_ctx.conn.query(
        \\SELECT status, rejection_reason, updated_at
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0002'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("OPERATOR_REJECTED", row.get([]const u8, 1) catch "");
    const updated_at = row.get(i64, 2) catch -1;
    try std.testing.expect(updated_at != 2_000); // second call must not have mutated updated_at
    try std.testing.expect((try q.next()) == null);
}

test "vetoAutoProposal returns false on already-vetoed proposal — no double mutation" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfileWithTrust(db_ctx.conn, "agent_idem_veto_1", "ws_idem_veto_1", 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, "agent_idem_veto_1", "ws_idem_veto_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0003");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0b0004', 'agent_idem_veto_1', 'ws_idem_veto_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0003', 'AUTO', 'READY', 'VETO_WINDOW', 20_000, 100, 101)
    , .{});

    const first = try proposals.vetoAutoProposal(
        db_ctx.conn,
        "agent_idem_veto_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0004",
        "OPERATOR_VETOED",
        1_000,
    );
    try std.testing.expect(first);

    // Proposal is now VETOED, not VETO_WINDOW — second call must return false.
    const second = try proposals.vetoAutoProposal(
        db_ctx.conn,
        "agent_idem_veto_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0004",
        "OPERATOR_VETOED",
        2_000,
    );
    try std.testing.expect(!second);

    var q = try db_ctx.conn.query(
        \\SELECT status
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0004'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("VETOED", row.get([]const u8, 0) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "rejectManualProposal returns false when proposal is already applied" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfile(db_ctx.conn, "agent_idem_rej_applied_1", "ws_idem_rej_applied_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_idem_rej_applied_1", "ws_idem_rej_applied_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0005");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0b0006', 'agent_idem_rej_applied_1', 'ws_idem_rej_applied_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0005', 'MANUAL', 'READY', 'APPLIED', 'operator:kishore', NULL, 100, 101)
    , .{});

    const result = try proposals.rejectManualProposal(
        db_ctx.conn,
        "agent_idem_rej_applied_1",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0006",
        "OPERATOR_REJECTED",
        3_000,
    );
    try std.testing.expect(!result);

    // Applied status must be unchanged.
    var q = try db_ctx.conn.query(
        \\SELECT status
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0006'
    , .{});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("APPLIED", row.get([]const u8, 0) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "listOpenProposals excludes proposals with generation_status not READY" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try support.createTempProposalTables(db_ctx.conn);
    try support.insertAgentProfile(db_ctx.conn, "agent_idem_list_1", "ws_idem_list_1");
    try support.insertActiveConfig(db_ctx.conn, "agent_idem_list_1", "ws_idem_list_1", "0195b4ba-8d3a-7f13-8abc-2b3e1e0b0007");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0b0008', 'agent_idem_list_1', 'ws_idem_list_1', 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0007', 'MANUAL', 'PENDING', 'PENDING_REVIEW', NULL, 100, 100),
        \\  ('0195b4ba-8d3a-7f13-8abc-2b3e1e0b0009', 'agent_idem_list_1', 'ws_idem_list_1', 'DECLINING_SCORE', '[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"agent_idem_list_1","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"quality"}]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0b0007', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 101, 101)
    , .{});

    const items = try proposals.listOpenProposals(db_ctx.conn, std.testing.allocator, "agent_idem_list_1", 0);
    defer {
        for (items) |*item| item.deinit(std.testing.allocator);
        std.testing.allocator.free(items);
    }

    // Only the READY proposal should be visible; the PENDING one must be excluded.
    try std.testing.expectEqual(@as(usize, 1), items.len);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0b0009", items[0].proposal_id);
}
