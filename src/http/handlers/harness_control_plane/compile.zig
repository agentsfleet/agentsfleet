const std = @import("std");
const pg = @import("pg");
const harness = @import("../../../harness/control_plane.zig");
const prompt_events = @import("../../../observability/prompt_events.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const entitlements = @import("../../../state/entitlements.zig");
const workspace_billing = @import("../../../state/workspace_billing.zig");
const workspace_credit = @import("../../../state/workspace_credit.zig");
const types = @import("types.zig");
const util = @import("util.zig");

const API_ACTOR = "api";
const COMPILE_ENGINE_DETERMINISTIC_V1 = "deterministic-v1";
const COMPILE_JOB_STATE_RUNNING = "RUNNING";
const COMPILE_JOB_STATE_SUCCEEDED = "SUCCEEDED";
const COMPILE_JOB_STATE_FAILED = "FAILED";
const VALIDATION_STATUS_RUNNING_JSON = "{\"status\":\"running\"}";
const COMPILE_FAILURE_REASON = "deterministic validation failed";

fn beginTx(conn: *pg.Conn) !void {
    var tx = try conn.query("BEGIN", .{});
    tx.deinit();
}

fn commitTx(conn: *pg.Conn) !void {
    var tx = try conn.query("COMMIT", .{});
    tx.deinit();
}

fn rollbackTx(conn: *pg.Conn) void {
    var tx = conn.query("ROLLBACK", .{}) catch return;
    tx.deinit();
}

pub fn compileProfile(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    workspace_id: []const u8,
    input: types.CompileInput,
) (types.ControlPlaneError || anyerror)!types.CompileOutput {
    if (input.agent_id) |provided_agent_id| {
        if (!util.isSupportedAgentId(provided_agent_id)) return types.ControlPlaneError.InvalidIdShape;
    }
    if (input.config_version_id) |provided| {
        if (!util.isSupportedConfigVersionId(provided)) return types.ControlPlaneError.InvalidIdShape;
    }

    const selection_sql = if (input.config_version_id != null)
        \\SELECT v.config_version_id, v.agent_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_config_versions v
        \\JOIN agent_profiles p ON p.agent_id = v.agent_id
        \\WHERE p.workspace_id = $1 AND v.config_version_id = $2
        \\LIMIT 1
    else if (input.agent_id != null)
        \\SELECT v.config_version_id, v.agent_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_config_versions v
        \\JOIN agent_profiles p ON p.agent_id = v.agent_id
        \\WHERE p.workspace_id = $1 AND v.agent_id = $2
        \\ORDER BY v.version DESC
        \\LIMIT 1
    else
        \\SELECT v.config_version_id, v.agent_id, v.version, v.source_markdown, p.tenant_id
        \\FROM agent_config_versions v
        \\JOIN agent_profiles p ON p.agent_id = v.agent_id
        \\WHERE p.workspace_id = $1
        \\ORDER BY v.created_at DESC
        \\LIMIT 1
    ;

    const selector_arg = input.config_version_id orelse input.agent_id orelse "";
    var pick = if (input.config_version_id == null and input.agent_id == null)
        try conn.query(selection_sql, .{workspace_id})
    else
        try conn.query(selection_sql, .{ workspace_id, selector_arg });
    defer pick.deinit();

    const row = (try pick.next()) orelse return types.ControlPlaneError.ProfileNotFound;
    const config_version_id = try row.get([]const u8, 0);
    const agent_id = try row.get([]const u8, 1);
    const version = try row.get(i32, 2);
    const source_markdown = try row.get([]const u8, 3);
    const tenant_id = try row.get([]const u8, 4);

    const compile_job_id = try util.generateCompileJobId(alloc);
    if (!util.isSupportedCompileJobId(compile_job_id)) return types.ControlPlaneError.InvalidIdShape;
    const now_ms = std.time.milliTimestamp();
    const billing_state = try workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, now_ms, API_ACTOR);
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |v| alloc.free(v);
    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, workspace_id, billing_state.plan_tier) catch |err| switch (err) {
        error.CreditExhausted => return types.ControlPlaneError.CreditExhausted,
        else => return err,
    };
    defer alloc.free(credit.currency);
    var outcome = harness.compileHarnessMarkdown(alloc, source_markdown) catch return types.ControlPlaneError.CompileFailed;
    defer outcome.deinit(alloc);
    entitlements.enforceWithAudit(
        conn,
        alloc,
        workspace_id,
        config_version_id,
        if (outcome.is_valid) outcome.compiled_profile_json else null,
        .compile,
        API_ACTOR,
    ) catch |err| switch (err) {
        entitlements.EnforcementError.EntitlementMissing => return types.ControlPlaneError.EntitlementMissing,
        entitlements.EnforcementError.EntitlementProfileLimit => return types.ControlPlaneError.EntitlementProfileLimit,
        entitlements.EnforcementError.EntitlementStageLimit => return types.ControlPlaneError.EntitlementStageLimit,
        entitlements.EnforcementError.EntitlementSkillNotAllowed => return types.ControlPlaneError.EntitlementSkillNotAllowed,
        entitlements.EnforcementError.InvalidCompiledProfile => return types.ControlPlaneError.CompileFailed,
        else => return err,
    };

    const finish_ts = std.time.milliTimestamp();
    try beginTx(conn);
    var tx_open = true;
    errdefer if (tx_open) rollbackTx(conn);

    var insert_job = try conn.query(
        \\INSERT INTO config_compile_jobs
        \\  (compile_job_id, tenant_id, workspace_id, requested_agent_id, requested_version, state, failure_reason, validation_report_json, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, $6, NULL, $7, $8, $8)
    , .{ compile_job_id, tenant_id, workspace_id, agent_id, version, COMPILE_JOB_STATE_RUNNING, VALIDATION_STATUS_RUNNING_JSON, now_ms });
    insert_job.deinit();

    var update_profile = try conn.query(
        \\UPDATE agent_config_versions
        \\SET compiled_profile_json = $1,
        \\    compile_engine = $2,
        \\    validation_report_json = $3,
        \\    is_valid = $4,
        \\    updated_at = $5
        \\WHERE config_version_id = $6
    , .{
        outcome.compiled_profile_json,
        COMPILE_ENGINE_DETERMINISTIC_V1,
        outcome.validation_report_json,
        outcome.is_valid,
        finish_ts,
        config_version_id,
    });
    update_profile.deinit();

    var update_job = try conn.query(
        \\UPDATE config_compile_jobs
        \\SET state = $1,
        \\    failure_reason = $2,
        \\    validation_report_json = $3,
        \\    updated_at = $4
        \\WHERE compile_job_id = $5
    , .{
        if (outcome.is_valid) COMPILE_JOB_STATE_SUCCEEDED else COMPILE_JOB_STATE_FAILED,
        if (outcome.is_valid) null else COMPILE_FAILURE_REASON,
        outcome.validation_report_json,
        finish_ts,
        compile_job_id,
    });
    update_job.deinit();

    if (outcome.is_valid) {
        const accepted_meta = std.fmt.allocPrint(alloc, "{{\"compile_job_id\":\"{s}\",\"version\":{d}}}", .{ compile_job_id, version }) catch "{}";
        prompt_events.emitBestEffort(conn, .{
            .event_type = .prompt_accepted,
            .workspace_id = workspace_id,
            .tenant_id = tenant_id,
            .agent_id = agent_id,
            .config_version_id = config_version_id,
            .metadata_json = accepted_meta,
            .ts_ms = finish_ts,
        });
    }

    try profile_linkage.insertCompileArtifact(
        conn,
        tenant_id,
        workspace_id,
        config_version_id,
        compile_job_id,
        outcome.is_valid,
        finish_ts,
    );

    try commitTx(conn);
    tx_open = false;

    return .{
        .compile_job_id = compile_job_id,
        .agent_id = agent_id,
        .config_version_id = config_version_id,
        .is_valid = outcome.is_valid,
        .validation_report_json = outcome.validation_report_json,
    };
}
