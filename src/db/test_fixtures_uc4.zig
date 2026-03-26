/// test_fixtures_uc4.zig — UC4: Run-start / runtime entitlement enforcement fixtures.
///
/// Covers: src/http/handlers/runs/start.zig integration tests
///
/// Tables seeded:
///   tenants, workspaces, workspace_entitlements,
///   agent_profiles, agent_config_versions, workspace_active_config
///
/// Tables cleaned via CASCADE (on workspace delete):
///   workspace_entitlements, workspace_active_config,
///   entitlement_policy_audit_snapshots
///
/// agent_profiles does NOT cascade from workspace — teardown deletes it explicitly
/// which then cascades to agent_config_versions.
///
/// Usage per test:
///
///   try uc4.seed(conn, std.testing.allocator);
///   defer uc4.teardown(conn);

const std = @import("std");
const base = @import("test_fixtures.zig");
const pg = @import("pg");

pub const TEST_TENANT_ID = base.TEST_TENANT_ID;

// UC4 identifiers — segment 5 prefix bb01 marks these as UC4 fixtures.
pub const WS_ID         = "0195b4ba-8d3a-7f13-8abc-bb0000000001";
pub const AGENT_ID      = "0195b4ba-8d3a-7f13-8abc-bb0000000011";
pub const CONFIG_VER_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000021";
pub const ENTITLEMENT_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000031";

// A 4-stage compiled profile — exceeds the FREE plan max_stages=3 limit.
// Gate stage is LAST (topology rule: GateStageMustBeLast), so validation passes
// and the stage-count enforcement fires as expected.
const SCALE_ONLY_PROFILE_JSON =
    \\{"agent_id":"0195b4ba-8d3a-7f13-8abc-bb0000000011","stages":[{"stage_id":"plan","role":"echo","skill":"echo"},{"stage_id":"implement","role":"scout","skill":"scout"},{"stage_id":"review","role":"scout","skill":"scout"},{"stage_id":"verify","role":"warden","skill":"warden","gate":true,"on_pass":"done","on_fail":"retry"}]}
;

/// Seed the full fixture set required for entitlement enforcement tests.
/// Insert order: tenant → workspace → entitlement → agent → config → active_config
pub fn seed(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS_ID);

    // FREE plan with max_stages=3 — the 4-stage profile will trip the limit.
    _ = try conn.exec(
        \\INSERT INTO workspace_entitlements
        \\  (entitlement_id, workspace_id, plan_tier, max_profiles, max_stages,
        \\   max_distinct_skills, allow_custom_skills, enable_agent_scoring,
        \\   agent_scoring_weights_json, created_at, updated_at)
        \\VALUES ($1, $2, 'FREE', 1, 3, 3, false, false,
        \\        '{"completion":0.4,"error_rate":0.3,"latency":0.2,"resource":0.1}',
        \\        0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ENTITLEMENT_ID, WS_ID });

    _ = try conn.exec(
        \\INSERT INTO agent_profiles
        \\  (agent_id, tenant_id, workspace_id, name, status,
        \\   trust_streak_runs, trust_level, last_scored_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, 'donald-duck', 'ACTIVE', 0, 'UNEARNED', NULL, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ AGENT_ID, TEST_TENANT_ID, WS_ID });

    _ = try conn.exec(
        \\INSERT INTO agent_config_versions
        \\  (config_version_id, tenant_id, agent_id, version, source_markdown,
        \\   compiled_profile_json, compile_engine, validation_report_json,
        \\   is_valid, created_at, updated_at)
        \\VALUES ($1, $2, $3, 1, $4, $4, 'deterministic-v1', '{}', TRUE, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ CONFIG_VER_ID, TEST_TENANT_ID, AGENT_ID, SCALE_ONLY_PROFILE_JSON });

    _ = try conn.exec(
        \\INSERT INTO workspace_active_config
        \\  (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
        \\VALUES ($1, $2, $3, 'test', 0)
        \\ON CONFLICT DO NOTHING
    , .{ WS_ID, TEST_TENANT_ID, CONFIG_VER_ID });
}

/// Tear down all UC4 fixtures.
///
/// FK constraint chain requires this exact order:
///   1. workspace_active_config (explicit) — holds NO ACTION FK to agent_config_versions;
///      must be removed before agent_profiles can cascade agent_config_versions.
///   2. agent_profiles — holds NO ACTION FK to workspaces; must be removed
///      before workspace can be deleted. Cascade removes agent_config_versions.
///   3. workspace — now safe; cascades workspace_entitlements, billing child tables,
///      entitlement_policy_audit_snapshots, etc.
///   4. tenant — all workspace FKs gone.
pub fn teardown(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM workspace_active_config WHERE workspace_id = $1::uuid",
        .{WS_ID},
    ) catch {};
    _ = conn.exec(
        "DELETE FROM agent_profiles WHERE agent_id = $1::uuid",
        .{AGENT_ID},
    ) catch {};
    base.teardownWorkspace(conn, WS_ID);
    base.teardownTenant(conn);
}
