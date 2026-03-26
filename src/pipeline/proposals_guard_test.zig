// Section 7.14 — HTTP-path guard acceptance tests.
// Verifies that applyProposal (the backend guard beneath the HTTP handlers)
// rejects apply calls when no matching proposal record exists in APPROVED or
// VETO_WINDOW status, and succeeds when a valid VETO_WINDOW proposal is present.
const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const auto_approval = @import("scoring_mod/proposals_auto_approval.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");
const base = @import("../db/test_fixtures.zig");
const uc2 = @import("../db/test_fixtures_uc2.zig");

// A valid stage_insert change JSON matching the 3-stage default profile.
const STAGE_INSERT_CHANGE =
    \\[{"target_field":"stage_insert","current_value":null,"proposed_value":{"agent_id":"0195b4ba-8d3a-7f13-8abc-dd0000000004","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},"rationale":"quality"}]
;

const WS_GUARD_1 = uc2.WS_GUARD_1;
const WS_GUARD_2 = "0195b4ba-8d3a-7f13-8abc-cc0000000401";
const WS_GUARD_3 = "0195b4ba-8d3a-7f13-8abc-cc0000000402";
const WS_GUARD_EDGE_1 = WS_GUARD_1;
const WS_GUARD_EDGE_2 = "0195b4ba-8d3a-7f13-8abc-cc0000000403";
const WS_GUARD_EDGE_3 = "0195b4ba-8d3a-7f13-8abc-cc0000000404";
const WS_GUARD_EDGE_4 = "0195b4ba-8d3a-7f13-8abc-cc0000000405";
const WS_GUARD_EDGE_5 = "0195b4ba-8d3a-7f13-8abc-cc0000000406";
const WS_GUARD_T5 = "0195b4ba-8d3a-7f13-8abc-cc0000000407";

test "applyProposal rejects when no proposal record exists for the given proposal_id" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_1);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_1);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_1);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_1, WS_GUARD_1, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_1, WS_GUARD_1, "0195b4ba-8d3a-7f13-8abc-3a0000000001");

    // No proposal row inserted — guard must reject.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_1,
        WS_GUARD_1,
        "0195b4ba-8d3a-7f13-8abc-ff0000000001",
        "0195b4ba-8d3a-7f13-8abc-3a0000000001",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        10_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = $1
    , .{uc2.AGENT_GUARD_1});
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

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_2);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_2);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_2);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_2, WS_GUARD_2, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_2, WS_GUARD_2, "0195b4ba-8d3a-7f13-8abc-3a0000000003");

    // Insert a MANUAL/PENDING_REVIEW proposal — wrong status for auto-apply guard.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000004', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000003', 'MANUAL', 'READY', 'PENDING_REVIEW', NULL, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_2, WS_GUARD_2, STAGE_INSERT_CHANGE });

    // Guard called with required_status=VETO_WINDOW — mismatch → rejected.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_2,
        WS_GUARD_2,
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
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = $1
    , .{uc2.AGENT_GUARD_2});
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

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_3);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_3);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_3);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000401', $1, 'SCALE', 10, 20, 10, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_GUARD_3});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_3, WS_GUARD_3, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_3, WS_GUARD_3, "0195b4ba-8d3a-7f13-8abc-3a0000000005");

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000006', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000005', 'AUTO', 'READY', 'VETO_WINDOW', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_3, WS_GUARD_3, STAGE_INSERT_CHANGE });

    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_3,
        WS_GUARD_3,
        "0195b4ba-8d3a-7f13-8abc-3a0000000006",
        "0195b4ba-8d3a-7f13-8abc-3a0000000005",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.applied, result);

    // harness_change_log must have exactly one row for this proposal.
    // Copy row-backed slices before query drain.
    var log_q = try db_ctx.conn.query(
        \\SELECT field_name, applied_by
        \\FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-3a0000000006'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const field_name_raw = try log_row.get([]const u8, 0);
    const field_name = try std.testing.allocator.dupe(u8, field_name_raw);
    defer std.testing.allocator.free(field_name);
    const applied_by_raw = try log_row.get([]const u8, 1);
    const applied_by = try std.testing.allocator.dupe(u8, applied_by_raw);
    defer std.testing.allocator.free(applied_by);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqualStrings("stage_insert", field_name);
    try std.testing.expectEqualStrings("system:auto", applied_by);

    // workspace_active_config must have been updated to a new config_version_id.
    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id::text
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
    , .{WS_GUARD_3});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    const new_version_raw = try active_row.get([]const u8, 0);
    const new_version_id = try std.testing.allocator.dupe(u8, new_version_raw);
    defer std.testing.allocator.free(new_version_id);
    try std.testing.expect((try active_q.next()) == null);
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-3a0000000005", new_version_id));
}

// ---------------------------------------------------------------------------
// T2 — Edge cases
// ---------------------------------------------------------------------------

test "applyProposal rejects when proposal_id is empty string" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_1);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_EDGE_1);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_1);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_EDGE_1, WS_GUARD_EDGE_1, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_EDGE_1, WS_GUARD_EDGE_1, "0195b4ba-8d3a-7f13-8abc-3a0000000010");

    // Call applyProposal with empty string proposal_id — guard must reject.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_EDGE_1,
        WS_GUARD_EDGE_1,
        "",
        "0195b4ba-8d3a-7f13-8abc-3a0000000010",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        10_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = $1
    , .{uc2.AGENT_GUARD_EDGE_1});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 0), log_count);
}

test "applyProposal rejects when agent_id does not match the proposal record" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_2);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_EDGE_2);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_2);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_EDGE_2, WS_GUARD_EDGE_2, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_EDGE_2, WS_GUARD_EDGE_2, "0195b4ba-8d3a-7f13-8abc-3a0000000011");

    // Insert proposal for AGENT_GUARD_EDGE_2.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000012', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000011', 'AUTO', 'READY', 'VETO_WINDOW', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_EDGE_2, WS_GUARD_EDGE_2, STAGE_INSERT_CHANGE });

    // Call with a DIFFERENT agent_id — the guard must reject because the
    // active config lookup uses workspace_id and any config_version mismatch
    // or the underlying proposal lookup will fail to match the given agent_id.
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        "0195b4ba-8d3a-7f13-8abc-dd000000ffff",
        WS_GUARD_EDGE_2,
        "0195b4ba-8d3a-7f13-8abc-3a0000000012",
        "0195b4ba-8d3a-7f13-8abc-3a0000000011",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);
}

test "applyProposal rejects when config_version_id does not match the proposal record" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_3);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_EDGE_3);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_3);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000402', $1, 'SCALE', 10, 20, 10, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_GUARD_EDGE_3});
    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_EDGE_3, WS_GUARD_EDGE_3, 10, "TRUSTED");
    // Config version A is the active config.
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_EDGE_3, WS_GUARD_EDGE_3, "0195b4ba-8d3a-7f13-8abc-3a0000000013");

    // Insert VETO_WINDOW proposal with config_version_id = A.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000014', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000013', 'AUTO', 'READY', 'VETO_WINDOW', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_EDGE_3, WS_GUARD_EDGE_3, STAGE_INSERT_CHANGE });

    // Call applyProposal with config_version_id = B (different from A in the proposal).
    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_EDGE_3,
        WS_GUARD_EDGE_3,
        "0195b4ba-8d3a-7f13-8abc-3a0000000014",
        "0195b4ba-8d3a-7f13-8abc-3a0000000099",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    // config_version mismatch → config_changed or rejected (either is a non-applied result).
    try std.testing.expect(result != .applied);
}

test "applyProposal rejects when proposal status is APPLIED (already applied)" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_4);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_EDGE_4);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_4);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_EDGE_4, WS_GUARD_EDGE_4, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_EDGE_4, WS_GUARD_EDGE_4, "0195b4ba-8d3a-7f13-8abc-3a0000000015");

    // Insert a proposal that is already APPLIED.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, applied_by, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000016', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000015', 'AUTO', 'READY', 'APPLIED', 'system:auto', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_EDGE_4, WS_GUARD_EDGE_4, STAGE_INSERT_CHANGE });

    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_EDGE_4,
        WS_GUARD_EDGE_4,
        "0195b4ba-8d3a-7f13-8abc-3a0000000016",
        "0195b4ba-8d3a-7f13-8abc-3a0000000015",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No additional harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = $1
    , .{uc2.AGENT_GUARD_EDGE_4});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 0), log_count);
}

test "applyProposal rejects when proposal status is VETOED" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_5);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_EDGE_5);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_EDGE_5);
    defer base.teardownTenant(db_ctx.conn);

    try support.insertAgentProfileWithTrust(db_ctx.conn, uc2.AGENT_GUARD_EDGE_5, WS_GUARD_EDGE_5, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_GUARD_EDGE_5, WS_GUARD_EDGE_5, "0195b4ba-8d3a-7f13-8abc-3a0000000017");

    // Insert a proposal that has been VETOED.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, rejection_reason, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000018', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000017', 'AUTO', 'READY', 'VETOED', 'OPERATOR_VETOED', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_GUARD_EDGE_5, WS_GUARD_EDGE_5, STAGE_INSERT_CHANGE });

    const result = try auto_approval.applyProposal(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_GUARD_EDGE_5,
        WS_GUARD_EDGE_5,
        "0195b4ba-8d3a-7f13-8abc-3a0000000018",
        "0195b4ba-8d3a-7f13-8abc-3a0000000017",
        STAGE_INSERT_CHANGE,
        "VETO_WINDOW",
        "system:auto",
        20_000,
    );

    try std.testing.expectEqual(auto_approval.ApplyProposalResult.rejected, result);

    // No harness_change_log row must have been written.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log WHERE agent_id = $1
    , .{uc2.AGENT_GUARD_EDGE_5});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 0), log_count);
}

// ---------------------------------------------------------------------------
// T5 — Sequential idempotency (concurrency guard)
// ---------------------------------------------------------------------------

test "applyProposal concurrent calls: only one succeeds, rest are rejected or idempotent" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_T5);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_GUARD_T5);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_GUARD_T5);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000403', $1, 'SCALE', 10, 20, 10, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_GUARD_T5});

    // agent_guard_t5 is a test-only agent ID (not in uc2); use a dedicated UUID
    const AGENT_GUARD_T5 = "0195b4ba-8d3a-7f13-8abc-dd00000000f0";
    try support.insertAgentProfileWithTrust(db_ctx.conn, AGENT_GUARD_T5, WS_GUARD_T5, 10, "TRUSTED");
    try support.insertActiveConfig(db_ctx.conn, AGENT_GUARD_T5, WS_GUARD_T5, "0195b4ba-8d3a-7f13-8abc-3a0000000020");

    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-3a0000000021', $1, $2, 'DECLINING_SCORE', $3, '0195b4ba-8d3a-7f13-8abc-3a0000000020', 'AUTO', 'READY', 'VETO_WINDOW', 10_000, 100, 101)
        \\ON CONFLICT DO NOTHING
    , .{ AGENT_GUARD_T5, WS_GUARD_T5, STAGE_INSERT_CHANGE });

    // Sequential idempotency: simulate 4 concurrent calls on the same connection.
    // The first call should succeed (.applied), the remaining 3 should be
    // rejected because the proposal is no longer in VETO_WINDOW status.
    var applied_count: u32 = 0;
    var rejected_count: u32 = 0;

    var call: usize = 0;
    while (call < 4) : (call += 1) {
        const call_result = try auto_approval.applyProposal(
            db_ctx.conn,
            std.testing.allocator,
            AGENT_GUARD_T5,
            WS_GUARD_T5,
            "0195b4ba-8d3a-7f13-8abc-3a0000000021",
            "0195b4ba-8d3a-7f13-8abc-3a0000000020",
            STAGE_INSERT_CHANGE,
            "VETO_WINDOW",
            "system:auto",
            20_000,
        );
        switch (call_result) {
            .applied => applied_count += 1,
            .rejected => rejected_count += 1,
            .config_changed => rejected_count += 1,
        }
    }

    // Exactly one call must have succeeded.
    try std.testing.expectEqual(@as(u32, 1), applied_count);
    // The remaining 3 must have been rejected (or config_changed, counted above).
    try std.testing.expectEqual(@as(u32, 3), rejected_count);

    // Exactly 1 harness_change_log row must exist for this proposal.
    var log_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM harness_change_log
        \\WHERE proposal_id = '0195b4ba-8d3a-7f13-8abc-3a0000000021'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    const log_count = try log_row.get(i64, 0);
    try std.testing.expect((try log_q.next()) == null);
    try std.testing.expectEqual(@as(i64, 1), log_count);
}
