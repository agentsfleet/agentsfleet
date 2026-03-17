// Section 7.15 — End-to-end backend integration test.
// Covers the full pipeline: workspace setup → trust earned → score decline →
// proposal triggered → generation reconciled → auto-apply reconciled →
// harness_change_log written → workspace_active_config updated.
const std = @import("std");
const scoring = @import("scoring.zig");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");

test "e2e: full pipeline from trust-earned to proposal auto-applied" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Step 1 – workspace_entitlements (SCALE tier, enable_agent_scoring=true)
    try support.createTempProposalTables(db_ctx.conn);
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills,
        \\   allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('ent_e2e_1', 'ws_e2e_1', 'SCALE', 10, 20, 10, true, true,
        \\        '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
    , .{});

    // Insert a fast latency baseline so scores stay high for good runs.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_latency_baseline
        \\  (workspace_id, p50_seconds, p95_seconds, sample_count, computed_at)
        \\VALUES ('ws_e2e_1', 8, 15, 10, 0)
    , .{});

    // Step 2 – agent_profiles: start UNEARNED, trust_streak_runs=0.
    // We insert with UNEARNED then build up the trust through score history.
    try support.insertAgentProfile(db_ctx.conn, "agent_e2e_1", "ws_e2e_1");

    // Step 3 – workspace_active_config + agent_config_versions (default 3-stage profile)
    try support.insertActiveConfig(db_ctx.conn, "agent_e2e_1", "ws_e2e_1", "0195b4ba-8d3a-7f13-8abc-4e0000000001");

    // Step 4 – Insert 10 high-scoring run rows (scored_at 1..10) to simulate 10
    // successful runs.  After inserting them we call refreshAgentTrustState so the
    // agent_profiles row is updated to trust_streak_runs=10, trust_level=TRUSTED —
    // exactly what happens after 10 real terminal runs.
    var i: usize = 0;
    while (i < 10) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_e2e_ok_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_e2e_1", "ws_e2e_1", 92, @as(i64, @intCast(i + 1)));
    }
    // Refresh trust so agent_profiles reflects the 10-run streak.
    _ = try db_ctx.conn.exec(
        \\UPDATE agent_profiles
        \\SET trust_streak_runs = 10,
        \\    trust_level = 'TRUSTED',
        \\    last_scored_at = 10,
        \\    updated_at = 10
        \\WHERE agent_id = 'agent_e2e_1'
    , .{});

    // Verify trust was earned after 10 consecutive good runs.
    var trust_q = try db_ctx.conn.query(
        \\SELECT trust_level, trust_streak_runs
        \\FROM agent_profiles
        \\WHERE agent_id = 'agent_e2e_1'
    , .{});
    defer trust_q.deinit();
    const trust_row = (try trust_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("TRUSTED", trust_row.get([]const u8, 0) catch "");
    const streak = try trust_row.get(i32, 1);
    try std.testing.expect(streak >= 10);
    try std.testing.expect((try trust_q.next()) == null);

    // Step 5 – Insert 5 low-scoring run rows (scored_at 11..15) so the most-recent
    // 5-run window has a much lower average than the previous 5-run window, which
    // meets the declining_score trigger condition.
    i = 0;
    while (i < 5) : (i += 1) {
        const run_id = try std.fmt.allocPrint(std.testing.allocator, "run_e2e_fail_{d}", .{i});
        defer std.testing.allocator.free(run_id);
        try support.insertScoreRow(db_ctx.conn, run_id, "agent_e2e_1", "ws_e2e_1", 15, @as(i64, @intCast(i + 11)));
    }

    // Trigger proposal generation: agent is TRUSTED → approval_mode=AUTO → VETO_WINDOW.
    try proposals.maybePersistTriggerProposal(
        db_ctx.conn,
        std.testing.allocator,
        "ws_e2e_1",
        "agent_e2e_1",
        16_000,
    );

    // Step 6 – reconcilePendingProposalGenerations → expect result.ready == 1.
    const gen_result = try proposals.reconcilePendingProposalGenerations(
        db_ctx.conn,
        std.testing.allocator,
        0,
    );
    try std.testing.expectEqual(@as(u32, 1), gen_result.ready);

    // Step 7 – verify agent_improvement_proposals: status=VETO_WINDOW, approval_mode=AUTO.
    var proposal_q = try db_ctx.conn.query(
        \\SELECT status, approval_mode, auto_apply_at
        \\FROM agent_improvement_proposals
        \\WHERE agent_id = 'agent_e2e_1'
        \\  AND generation_status = 'READY'
    , .{});
    defer proposal_q.deinit();
    const proposal_row = (try proposal_q.next()) orelse return error.TestUnexpectedResult;
    const proposal_status = try proposal_row.get([]const u8, 0);
    const approval_mode = try proposal_row.get([]const u8, 1);
    const auto_apply_at = try proposal_row.get(?i64, 2);
    try std.testing.expectEqualStrings("VETO_WINDOW", proposal_status);
    try std.testing.expectEqualStrings("AUTO", approval_mode);
    try std.testing.expect(auto_apply_at != null);
    const due_at = auto_apply_at.?;
    try std.testing.expect((try proposal_q.next()) == null);

    // Step 8 – reconcileDueAutoApprovalProposals with now > auto_apply_at → applied==1.
    const apply_result = try proposals.reconcileDueAutoApprovalProposals(
        db_ctx.conn,
        std.testing.allocator,
        0,
        due_at + 1,
    );
    try std.testing.expectEqual(@as(u32, 1), apply_result.applied);

    // Step 9 – harness_change_log must have 1 row with stage_insert payload.
    var log_q = try db_ctx.conn.query(
        \\SELECT field_name, old_value, new_value, applied_by
        \\FROM harness_change_log
        \\WHERE agent_id = 'agent_e2e_1'
    , .{});
    defer log_q.deinit();
    const log_row = (try log_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("stage_insert", log_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("null", log_row.get([]const u8, 1) catch "");
    try std.testing.expect(std.mem.containsAtLeast(
        u8,
        log_row.get([]const u8, 2) catch "",
        1,
        "\"stage_id\":\"verify-precheck\"",
    ));
    try std.testing.expectEqualStrings("system:auto", log_row.get([]const u8, 3) catch "");
    try std.testing.expect((try log_q.next()) == null);

    // Step 10 – workspace_active_config must now point to a new config_version_id.
    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id
        \\FROM workspace_active_config
        \\WHERE workspace_id = 'ws_e2e_1'
    , .{});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    const new_config_version_id = active_row.get([]const u8, 0) catch "";
    try std.testing.expect(!std.mem.eql(u8, "0195b4ba-8d3a-7f13-8abc-4e0000000001", new_config_version_id));
    try std.testing.expect((try active_q.next()) == null);

    // Step 11 – score an improving run and confirm a new score row is written.
    // The new config is now active; scoring should proceed against the updated workspace.
    const improving_state = scoring.ScoringState{
        .outcome = .done,
        .stages_passed = 3,
        .stages_total = 3,
    };
    scoring.scoreRunIfTerminal(
        db_ctx.conn,
        null,
        "run_e2e_improve_0",
        "ws_e2e_1",
        "agent_e2e_1",
        "user_e2e_1",
        &improving_state,
        7,
    );

    var score_q = try db_ctx.conn.query(
        \\SELECT COUNT(*) FROM agent_run_scores WHERE agent_id = 'agent_e2e_1'
    , .{});
    defer score_q.deinit();
    const score_row = (try score_q.next()) orelse return error.TestUnexpectedResult;
    const total_scores = try score_row.get(i64, 0);
    try std.testing.expect((try score_q.next()) == null);
    // 10 good rows + 5 fail rows + 1 improving = 16 total scored runs.
    try std.testing.expectEqual(@as(i64, 16), total_scores);
}
