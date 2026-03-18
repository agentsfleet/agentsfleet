const std = @import("std");
const pg = @import("pg");
const prompt_events = @import("../../../observability/prompt_events.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const entitlements = @import("../../../state/entitlements.zig");
const workspace_billing = @import("../../../state/workspace_billing.zig");
const workspace_credit = @import("../../../state/workspace_credit.zig");
const types = @import("types.zig");
const util = @import("util.zig");

fn beginTx(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
}

fn commitTx(conn: *pg.Conn) !void {
    _ = try conn.exec("COMMIT", .{});
}

fn rollbackTx(conn: *pg.Conn) void {
    _ = conn.exec("ROLLBACK", .{}) catch {};
}

pub fn activateProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: types.ActivateInput,
) (types.ControlPlaneError || anyerror)!types.ActivateOutput {
    if (!util.isSupportedConfigVersionId(input.config_version_id)) return types.ControlPlaneError.InvalidIdShape;

    var q = try conn.query(
        \\SELECT v.agent_id, v.is_valid, p.tenant_id, v.compiled_profile_json
        \\FROM agent_config_versions v
        \\JOIN agent_profiles p ON p.agent_id = v.agent_id
        \\WHERE p.workspace_id = $1 AND v.config_version_id = $2
        \\LIMIT 1
    , .{ workspace_id, input.config_version_id });
    defer q.deinit();

    const row = (try q.next()) orelse return types.ControlPlaneError.ProfileNotFound;
    const agent_id = try row.get([]const u8, 0);
    const is_valid = try row.get(bool, 1);
    const tenant_id = try row.get([]const u8, 2);
    const compiled_profile_json = try row.get(?[]const u8, 3);
    if (!is_valid) return types.ControlPlaneError.ProfileInvalid;
    const billing_state = try workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, std.time.milliTimestamp(), input.activated_by orelse "api");
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |v| alloc.free(v);
    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, workspace_id, billing_state.plan_tier) catch |err| switch (err) {
        error.CreditExhausted => return types.ControlPlaneError.CreditExhausted,
        else => return err,
    };
    defer alloc.free(credit.currency);
    entitlements.enforceWithAudit(
        conn,
        alloc,
        workspace_id,
        input.config_version_id,
        compiled_profile_json,
        .activate,
        input.activated_by orelse "api",
    ) catch |err| switch (err) {
        entitlements.EnforcementError.EntitlementMissing => return types.ControlPlaneError.EntitlementMissing,
        entitlements.EnforcementError.EntitlementProfileLimit => return types.ControlPlaneError.EntitlementProfileLimit,
        entitlements.EnforcementError.EntitlementStageLimit => return types.ControlPlaneError.EntitlementStageLimit,
        entitlements.EnforcementError.EntitlementSkillNotAllowed => return types.ControlPlaneError.EntitlementSkillNotAllowed,
        entitlements.EnforcementError.InvalidCompiledProfile => return types.ControlPlaneError.ProfileInvalid,
        else => return err,
    };

    const now_ms = std.time.milliTimestamp();
    const activated_by = input.activated_by orelse "api";

    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

    var upsert = try conn.query(
        \\INSERT INTO workspace_active_config (workspace_id, tenant_id, config_version_id, activated_by, activated_at)
        \\VALUES ($1, $2, $3, $4, $5)
        \\ON CONFLICT (workspace_id) DO UPDATE
        \\SET tenant_id = EXCLUDED.tenant_id,
        \\    config_version_id = EXCLUDED.config_version_id,
        \\    activated_by = EXCLUDED.activated_by,
        \\    activated_at = EXCLUDED.activated_at
    , .{ workspace_id, tenant_id, input.config_version_id, activated_by, now_ms });
    upsert.deinit();

    var mark_active = try conn.query(
        "UPDATE agent_profiles SET status = CASE WHEN agent_id = $1 THEN 'ACTIVE' ELSE status END, updated_at = $2 WHERE workspace_id = $3",
        .{ agent_id, now_ms, workspace_id },
    );
    mark_active.deinit();

    prompt_events.emitBestEffort(conn, .{
        .event_type = .prompt_applied,
        .workspace_id = workspace_id,
        .tenant_id = tenant_id,
        .agent_id = agent_id,
        .config_version_id = input.config_version_id,
        .metadata_json = "{}",
        .ts_ms = now_ms,
    });
    try profile_linkage.insertActivateArtifact(conn, tenant_id, workspace_id, input.config_version_id, activated_by, now_ms);
    try commitTx(conn);
    tx_open = false;

    return .{
        .agent_id = agent_id,
        .config_version_id = input.config_version_id,
        .run_snapshot_config_version = input.config_version_id,
        .activated_by = activated_by,
        .activated_at = now_ms,
    };
}
