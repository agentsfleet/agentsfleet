const std = @import("std");
const proposals = @import("scoring_mod/proposals.zig");
const common = @import("../http/handlers/common.zig");
const support = @import("proposals_test_support.zig");
const base = @import("../db/test_fixtures.zig");
const uc2 = @import("../db/test_fixtures_uc2.zig");

const WS_REVERT_125 = "0195b4ba-8d3a-7f13-8abc-cc0000000125";
const WS_REVERT_124 = "0195b4ba-8d3a-7f13-8abc-cc0000000124";
const WS_REVERT_108 = "0195b4ba-8d3a-7f13-8abc-cc0000000108";
const WS_REVERT_103 = "0195b4ba-8d3a-7f13-8abc-cc0000000103";

// Fixed UUID for the stage-limit proposal used in test 3.
const PROP_STAGE_LIMIT_1 = "0195b4ba-8d3a-7f13-8abc-ee0010000108";

test "revertHarnessChange restores previous stage_insert profile and audit log" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const profile_before =
        \\{"agent_id":"agent_prop_revert_insert","stages":[
        \\{"stage_id":"plan","role":"echo","skill":"echo","artifact_name":"plan.json","commit_message":"echo: add plan.json","gate":false},
        \\{"stage_id":"implement","role":"scout","skill":"scout","artifact_name":"implementation.md","commit_message":"scout: add implementation.md","gate":false},
        \\{"stage_id":"verify","role":"warden","skill":"warden","artifact_name":"validation.md","commit_message":"warden: add validation.md","gate":true,"on_pass":"done","on_fail":"retry"}]}
    ;
    const profile_after =
        \\{"agent_id":"agent_prop_revert_insert","stages":[
        \\{"stage_id":"plan","role":"echo","skill":"echo","artifact_name":"plan.json","commit_message":"echo: add plan.json","gate":false},
        \\{"stage_id":"implement","role":"scout","skill":"scout","artifact_name":"implementation.md","commit_message":"scout: add implementation.md","gate":false},
        \\{"stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"},
        \\{"stage_id":"verify","role":"warden","skill":"warden","artifact_name":"validation.md","commit_message":"warden: add validation.md","gate":true,"on_pass":"done","on_fail":"retry"}]}
    ;
    const inserted_stage =
        \\{"agent_id":"agent_prop_revert_insert","insert_before_stage_id":"verify","stage_id":"verify-precheck","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-precheck.md","commit_message":"agent: add verify-precheck.md","gate":false,"on_pass":"verify","on_fail":"retry"}
    ;

    uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_125);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_REVERT_125);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_125);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000125', $1, 'SCALE', 8, 8, 8, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_REVERT_125});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_REVERT_INSERT, WS_REVERT_125);
    try support.insertActiveConfigWithProfile(
        db_ctx.conn,
        uc2.AGENT_PROP_REVERT_INSERT,
        WS_REVERT_125,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff1",
        profile_before,
    );
    try support.insertConfigVersionOnly(
        db_ctx.conn,
        uc2.AGENT_PROP_REVERT_INSERT,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff2",
        2,
        profile_after,
    );
    _ = try db_ctx.conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff2',
        \\    activated_by = 'operator:kishore',
        \\    activated_at = 1
        \\WHERE workspace_id = $1
    , .{WS_REVERT_125});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, applied_by, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff3', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff1', 'MANUAL', 'READY', 'APPLIED', NULL, 'operator:kishore', 0, 1)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_REVERT_INSERT, WS_REVERT_125 });
    _ = try db_ctx.conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4', $1, '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff3', $2, 'stage_insert', 'null', $3, 1, 'operator:kishore', NULL)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_REVERT_INSERT, WS_REVERT_125, inserted_stage });

    var result = (try proposals.revertHarnessChange(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_PROP_REVERT_INSERT,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4",
        "kishore",
        6_000,
    )) orelse return error.TestUnexpectedResult;
    defer result.deinit(std.testing.allocator);

    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4", result.reverted_from);
    try std.testing.expectEqualStrings("operator:kishore", result.applied_by);

    var active_q = try db_ctx.conn.query(
        \\SELECT config_version_id::text, activated_by
        \\FROM workspace_active_config
        \\WHERE workspace_id = $1
    , .{WS_REVERT_125});
    defer active_q.deinit();
    const active_row = (try active_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings(result.config_version_id, active_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("operator:kishore", active_row.get([]const u8, 1) catch "");
    try std.testing.expect((try active_q.next()) == null);

    var config_q = try db_ctx.conn.query(
        \\SELECT compiled_profile_json
        \\FROM agent_config_versions
        \\WHERE config_version_id = $1
    , .{result.config_version_id});
    defer config_q.deinit();
    const config_row = (try config_q.next()) orelse return error.TestUnexpectedResult;
    try support.expectProfileMatches(config_row.get([]const u8, 0) catch "", "agent_prop_revert_insert", &.{
        .{
            .stage_id = "plan",
            .role_id = "echo",
            .skill_id = "echo",
            .artifact_name = "plan.json",
            .commit_message = "echo: add plan.json",
            .is_gate = false,
            .on_pass = null,
            .on_fail = null,
        },
        .{
            .stage_id = "implement",
            .role_id = "scout",
            .skill_id = "scout",
            .artifact_name = "implementation.md",
            .commit_message = "scout: add implementation.md",
            .is_gate = false,
            .on_pass = null,
            .on_fail = null,
        },
        .{
            .stage_id = "verify",
            .role_id = "warden",
            .skill_id = "warden",
            .artifact_name = "validation.md",
            .commit_message = "warden: add validation.md",
            .is_gate = true,
            .on_pass = "done",
            .on_fail = "retry",
        },
    });
    try std.testing.expect((try config_q.next()) == null);

    var change_q = try db_ctx.conn.query(
        \\SELECT field_name, old_value, new_value, applied_by, reverted_from::text
        \\FROM harness_change_log
        \\WHERE change_id = $1
    , .{result.change_id});
    defer change_q.deinit();
    const change_row = (try change_q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("stage_insert", change_row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings(inserted_stage, change_row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("null", change_row.get([]const u8, 2) catch "");
    try std.testing.expectEqualStrings("operator:kishore", change_row.get([]const u8, 3) catch "");
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6ff4", change_row.get([]const u8, 4) catch "");
    try std.testing.expect((try change_q.next()) == null);
}

test "revertHarnessChange restores previous stage_binding profile exactly" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const profile_before =
        \\{"agent_id":"agent_prop_revert_binding","stages":[
        \\{"stage_id":"plan","role":"echo","skill":"echo","artifact_name":"plan.json","commit_message":"echo: add plan.json","gate":false},
        \\{"stage_id":"implement","role":"scout","skill":"scout","artifact_name":"implementation.md","commit_message":"scout: add implementation.md","gate":false},
        \\{"stage_id":"verify","role":"warden","skill":"warden","artifact_name":"validation.md","commit_message":"warden: add validation.md","gate":true,"on_pass":"done","on_fail":"retry"}]}
    ;
    const profile_after =
        \\{"agent_id":"agent_prop_revert_binding","stages":[
        \\{"stage_id":"plan","role":"echo","skill":"echo","artifact_name":"plan.json","commit_message":"echo: add plan.json","gate":false},
        \\{"stage_id":"implement","role":"scout","skill":"scout","artifact_name":"implementation.md","commit_message":"scout: add implementation.md","gate":false},
        \\{"stage_id":"verify","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-ready.md","commit_message":"agent: replace verify-ready.md","gate":true,"on_pass":"done","on_fail":"retry"}]}
    ;
    const old_stage =
        \\{"stage_id":"verify","role":"warden","skill":"warden","artifact_name":"validation.md","commit_message":"warden: add validation.md","gate":true,"on_pass":"done","on_fail":"retry"}
    ;
    const new_stage =
        \\{"stage_id":"verify","role":"autoworkerready","skill":"clawhub://usezombie/autoworkerready@1.0.0","artifact_name":"verify-ready.md","commit_message":"agent: replace verify-ready.md","gate":true,"on_pass":"done","on_fail":"retry"}
    ;

    uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_124);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_REVERT_124);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_124);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000124', $1, 'SCALE', 8, 8, 8, true, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_REVERT_124});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_REVERT_BINDING, WS_REVERT_124);
    try support.insertActiveConfigWithProfile(
        db_ctx.conn,
        uc2.AGENT_PROP_REVERT_BINDING,
        WS_REVERT_124,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001",
        profile_before,
    );
    try support.insertConfigVersionOnly(
        db_ctx.conn,
        uc2.AGENT_PROP_REVERT_BINDING,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002",
        2,
        profile_after,
    );
    _ = try db_ctx.conn.exec(
        \\UPDATE workspace_active_config
        \\SET config_version_id = '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7002',
        \\    activated_by = 'operator:kishore',
        \\    activated_at = 1
        \\WHERE workspace_id = $1
    , .{WS_REVERT_124});
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, applied_by, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7003', $1, $2, 'DECLINING_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7001', 'MANUAL', 'READY', 'APPLIED', NULL, 'operator:kishore', 0, 1)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_REVERT_BINDING, WS_REVERT_124 });
    _ = try db_ctx.conn.exec(
        \\INSERT INTO harness_change_log
        \\  (change_id, agent_id, proposal_id, workspace_id, field_name, old_value, new_value, applied_at, applied_by, reverted_from)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a7004', $1, '0195b4ba-8d3a-7f13-8abc-2b3e1e0a7003', $2, 'stage_binding', $3, $4, 1, 'operator:kishore', NULL)
        \\ON CONFLICT DO NOTHING
    , .{ uc2.AGENT_PROP_REVERT_BINDING, WS_REVERT_124, old_stage, new_stage });

    var result = (try proposals.revertHarnessChange(
        db_ctx.conn,
        std.testing.allocator,
        uc2.AGENT_PROP_REVERT_BINDING,
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a7004",
        "kishore",
        7_000,
    )) orelse return error.TestUnexpectedResult;
    defer result.deinit(std.testing.allocator);

    var config_q = try db_ctx.conn.query(
        \\SELECT compiled_profile_json
        \\FROM agent_config_versions
        \\WHERE config_version_id = $1
    , .{result.config_version_id});
    defer config_q.deinit();
    const config_row = (try config_q.next()) orelse return error.TestUnexpectedResult;
    try support.expectProfileMatches(config_row.get([]const u8, 0) catch "", "agent_prop_revert_binding", &.{
        .{
            .stage_id = "plan",
            .role_id = "echo",
            .skill_id = "echo",
            .artifact_name = "plan.json",
            .commit_message = "echo: add plan.json",
            .is_gate = false,
            .on_pass = null,
            .on_fail = null,
        },
        .{
            .stage_id = "implement",
            .role_id = "scout",
            .skill_id = "scout",
            .artifact_name = "implementation.md",
            .commit_message = "scout: add implementation.md",
            .is_gate = false,
            .on_pass = null,
            .on_fail = null,
        },
        .{
            .stage_id = "verify",
            .role_id = "warden",
            .skill_id = "warden",
            .artifact_name = "validation.md",
            .commit_message = "warden: add validation.md",
            .is_gate = true,
            .on_pass = "done",
            .on_fail = "retry",
        },
    });
    try std.testing.expect((try config_q.next()) == null);
}

test "reconcilePendingProposalGenerations rejects generated proposals that exceed stage entitlements" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_108);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_REVERT_108);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_108);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000108', $1, 'FREE', 3, 3, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_REVERT_108});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_5, WS_REVERT_108);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_5, WS_REVERT_108, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95");
    _ = try db_ctx.conn.exec(
        \\INSERT INTO agent_improvement_proposals
        \\  (proposal_id, agent_id, workspace_id, trigger_reason, proposed_changes, config_version_id, approval_mode, generation_status, status, auto_apply_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'SUSTAINED_LOW_SCORE', '[]', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f95', 'MANUAL', 'PENDING', 'PENDING_REVIEW', NULL, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ PROP_STAGE_LIMIT_1, uc2.AGENT_PROP_5, WS_REVERT_108 });

    const result = try proposals.reconcilePendingProposalGenerations(db_ctx.conn, std.testing.allocator, 0);
    try std.testing.expectEqual(@as(u32, 0), result.ready);
    try std.testing.expectEqual(@as(u32, 1), result.rejected);

    var q = try db_ctx.conn.query(
        \\SELECT proposed_changes, generation_status, status, rejection_reason
        \\FROM agent_improvement_proposals
        \\WHERE proposal_id = $1
    , .{PROP_STAGE_LIMIT_1});
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    try std.testing.expectEqualStrings("[]", row.get([]const u8, 0) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 1) catch "");
    try std.testing.expectEqualStrings("REJECTED", row.get([]const u8, 2) catch "");
    try std.testing.expectEqualStrings("UZ-ENTL-003", row.get([]const u8, 3) catch "");
    try std.testing.expect((try q.next()) == null);
}

test "proposal validation rejects unregistered agent refs and entitlement-disallowed skills" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_103);
    try base.seedTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, WS_REVERT_103);
    defer uc2.teardownWorkspace(db_ctx.conn, WS_REVERT_103);
    defer base.teardownTenant(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages, max_distinct_skills, allow_custom_skills, enable_agent_scoring, agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ('0195b4ba-8d3a-7f13-8abc-ee0000000103', $1, 'FREE', 3, 4, 3, false, true, '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{WS_REVERT_103});
    try support.insertAgentProfile(db_ctx.conn, uc2.AGENT_PROP_3, WS_REVERT_103);
    try support.insertActiveConfig(db_ctx.conn, uc2.AGENT_PROP_3, WS_REVERT_103, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f96");

    // Test 1: unknown agent UUID returns UnregisteredAgentRef.
    try std.testing.expectError(
        proposals.ProposalValidationError.UnregisteredAgentRef,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            WS_REVERT_103,
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"0195b4ba-8d3a-7f13-8abc-ffffff000001\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"warden\"},\"rationale\":\"rebind stage\"}]",
        ),
    );

    // Test 2: known agent with non-whitelisted custom skill returns EntitlementSkillNotAllowed.
    try std.testing.expectError(
        proposals.ProposalValidationError.EntitlementSkillNotAllowed,
        proposals.validateProposedChanges(
            db_ctx.conn,
            std.testing.allocator,
            WS_REVERT_103,
            "[{\"target_field\":\"stage_binding\",\"proposed_value\":{\"agent_id\":\"" ++ uc2.AGENT_PROP_3 ++ "\",\"stage_id\":\"verify\",\"role\":\"warden\",\"skill\":\"clawhub://openclaw/github-reviewer@1.2.0\"},\"rationale\":\"rebind stage\"}]",
        ),
    );
}
