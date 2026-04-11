const std = @import("std");
const httpz = @import("httpz");
const posthog_events = @import("../../observability/posthog_events.zig");
const error_codes = @import("../../errors/codes.zig");
const workspace_guards = @import("../workspace_guards.zig");
const harness_handlers = @import("harness_control_plane.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub const Context = common.Context;

fn innerPutHarnessSource(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.PutSourceInput;
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    // putSource is the ingestion step that costs credits — it writes new harness
    // data into the workspace. compile and activate operate on already-ingested
    // data, so they use the default credit_policy (.none).
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
        .credit_policy = .execution_required,
    }) orelse return;
    defer access.deinit(hx.alloc);

    const out = harness_handlers.putSource(conn, hx.alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => hx.fail(error_codes.ERR_INVALID_REQUEST, "Invalid harness source payload"),
            error.InvalidIdShape => hx.fail(error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid agent_id format"),
            error.WorkspaceNotFound => hx.fail(error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found"),
            else => common.internalOperationError(hx.res, "Failed to store harness source", hx.req_id),
        }
        log.err("harness.put_source_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        return;
    };

    log.info("harness.source_stored workspace_id={s} agent_id={s}", .{ workspace_id, out.agent_id });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .version = out.version,
        .status = "DRAFT",
        .request_id = hx.req_id,
    });
}

pub const handlePutHarnessSource = hx_mod.authenticatedWithParam(innerPutHarnessSource);

fn innerCompileHarness(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.CompileInput;
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("harness.compile workspace_id={s}", .{workspace_id});

    const out = harness_handlers.compileProfile(conn, hx.alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidIdShape => hx.fail(error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid config_version_id format"),
            error.ProfileNotFound => hx.fail(error_codes.ERR_PROFILE_NOT_FOUND, "No harness profile source found for workspace"),
            error.CompileFailed => common.internalOperationError(hx.res, "Harness compile failed", hx.req_id),
            error.EntitlementMissing => {
                posthog_events.trackEntitlementRejected(hx.ctx.posthog, posthog_events.distinctIdOrSystem(hx.principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, hx.req_id);
                hx.fail(error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied");
            },
            error.EntitlementProfileLimit => {
                posthog_events.trackEntitlementRejected(hx.ctx.posthog, posthog_events.distinctIdOrSystem(hx.principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, hx.req_id);
                hx.fail(error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded");
            },
            error.EntitlementStageLimit => {
                posthog_events.trackEntitlementRejected(hx.ctx.posthog, posthog_events.distinctIdOrSystem(hx.principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, hx.req_id);
                hx.fail(error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded");
            },
            error.EntitlementSkillNotAllowed => {
                posthog_events.trackEntitlementRejected(hx.ctx.posthog, posthog_events.distinctIdOrSystem(hx.principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, hx.req_id);
                hx.fail(error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills");
            },
            error.CreditExhausted => hx.fail(error_codes.ERR_CREDIT_EXHAUSTED, "Free plan credit exhausted. Upgrade to Scale to continue."),
            else => common.internalOperationError(hx.res, "Failed to compile harness profile", hx.req_id),
        }
        log.err("harness.compile_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        return;
    };

    log.info("harness.compiled workspace_id={s} agent_id={s} is_valid={}", .{ workspace_id, out.agent_id, out.is_valid });

    hx.ok(.ok, .{
        .compile_job_id = out.compile_job_id,
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .is_valid = out.is_valid,
        .validation_report_json = out.validation_report_json,
        .request_id = hx.req_id,
    });
}

pub const handleCompileHarness = hx_mod.authenticatedWithParam(innerCompileHarness);

fn reportActivateError(hx: hx_mod.Hx, workspace_id: []const u8, err: anyerror) void {
    const distinct_id = posthog_events.distinctIdOrSystem(hx.principal.user_id orelse "");
    switch (err) {
        error.InvalidIdShape => hx.fail(error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid config_version_id format"),
        error.ProfileNotFound => hx.fail(error_codes.ERR_PROFILE_NOT_FOUND, "Profile version not found"),
        error.ProfileInvalid => hx.fail(error_codes.ERR_PROFILE_INVALID, "Invalid profile cannot be activated"),
        error.EntitlementMissing => {
            posthog_events.trackEntitlementRejected(hx.ctx.posthog, distinct_id, workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, hx.req_id);
            hx.fail(error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied");
        },
        error.EntitlementProfileLimit => {
            posthog_events.trackEntitlementRejected(hx.ctx.posthog, distinct_id, workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, hx.req_id);
            hx.fail(error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded");
        },
        error.EntitlementStageLimit => {
            posthog_events.trackEntitlementRejected(hx.ctx.posthog, distinct_id, workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, hx.req_id);
            hx.fail(error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded");
        },
        error.EntitlementSkillNotAllowed => {
            posthog_events.trackEntitlementRejected(hx.ctx.posthog, distinct_id, workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, hx.req_id);
            hx.fail(error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills");
        },
        error.CreditExhausted => hx.fail(error_codes.ERR_CREDIT_EXHAUSTED, "Free plan credit exhausted. Upgrade to Scale to continue."),
        else => common.internalOperationError(hx.res, "Failed to activate profile", hx.req_id),
    }
    log.err("harness.activate_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
}

fn innerActivateHarness(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.ActivateInput;
    const body = req.body() orelse {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(Req, hx.alloc, body, .{}) catch {
        hx.fail(error_codes.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("harness.activate workspace_id={s}", .{workspace_id});

    const out = harness_handlers.activateProfile(conn, hx.alloc, workspace_id, parsed.value) catch |err| {
        reportActivateError(hx, workspace_id, err);
        return;
    };

    log.info("harness.activated workspace_id={s} agent_id={s} config_version_id={s}", .{ workspace_id, out.agent_id, out.config_version_id });

    posthog_events.trackProfileActivated(
        hx.ctx.posthog,
        posthog_events.distinctIdOrSystem(hx.principal.user_id orelse ""),
        workspace_id,
        out.agent_id,
        out.config_version_id,
        out.run_snapshot_config_version,
        hx.req_id,
    );

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .run_snapshot_config_version = out.run_snapshot_config_version,
        .activated_by = out.activated_by,
        .activated_at = out.activated_at,
        .request_id = hx.req_id,
    });
}

pub const handleActivateHarness = hx_mod.authenticatedWithParam(innerActivateHarness);

fn innerGetHarnessActive(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    _ = req;
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, hx.principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    log.debug("harness.get_active workspace_id={s}", .{workspace_id});

    const out = harness_handlers.getActiveProfile(conn, hx.alloc, workspace_id) catch {
        log.err("harness.get_active_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        common.internalOperationError(hx.res, "Failed to resolve active profile", hx.req_id);
        return;
    };
    defer hx.alloc.free(out.profile_json);

    const parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, out.profile_json, .{}) catch {
        common.internalOperationError(hx.res, "Failed to render profile JSON", hx.req_id);
        return;
    };
    defer parsed.deinit();
    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .source = out.source,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .run_snapshot_config_version = out.run_snapshot_config_version,
        .active_at = out.active_at,
        .profile = parsed.value,
        .request_id = hx.req_id,
    });
}

pub const handleGetHarnessActive = hx_mod.authenticatedWithParam(innerGetHarnessActive);
