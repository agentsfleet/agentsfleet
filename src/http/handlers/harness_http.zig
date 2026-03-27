const std = @import("std");
const httpz = @import("httpz");
const posthog_events = @import("../../observability/posthog_events.zig");
const error_codes = @import("../../errors/codes.zig");
const workspace_guards = @import("../workspace_guards.zig");
const harness_handlers = @import("harness_control_plane.zig");
const common = @import("common.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub const Context = common.Context;

pub fn handlePutHarnessSource(ctx: *Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.PutSourceInput;
    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    // putSource is the ingestion step that costs credits — it writes new harness
    // data into the workspace.  compile and activate operate on already-ingested
    // data, so they use the default credit_policy (.none).
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
        .credit_policy = .execution_required,
    }) orelse return;
    defer access.deinit(alloc);

    const out = harness_handlers.putSource(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Invalid harness source payload", req_id),
            error.InvalidIdShape => common.errorResponse(res, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid agent_id format", req_id),
            error.WorkspaceNotFound => common.errorResponse(res, .not_found, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id),
            else => common.internalOperationError(res, "Failed to store harness source", req_id),
        }
        log.err("harness.put_source_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        return;
    };

    log.info("harness.source_stored workspace_id={s} agent_id={s}", .{ workspace_id, out.agent_id });

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .version = out.version,
        .status = "DRAFT",
        .request_id = req_id,
    });
}

pub fn handleCompileHarness(ctx: *Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.CompileInput;
    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    log.debug("harness.compile workspace_id={s}", .{workspace_id});

    const out = harness_handlers.compileProfile(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidIdShape => common.errorResponse(res, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid config_version_id format", req_id),
            error.ProfileNotFound => common.errorResponse(res, .not_found, error_codes.ERR_PROFILE_NOT_FOUND, "No harness profile source found for workspace", req_id),
            error.CompileFailed => common.internalOperationError(res, "Harness compile failed", req_id),
            error.EntitlementMissing => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied", req_id);
            },
            error.EntitlementProfileLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded", req_id);
            },
            error.EntitlementStageLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded", req_id);
            },
            error.EntitlementSkillNotAllowed => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills", req_id);
            },
            error.CreditExhausted => common.errorResponse(res, .forbidden, error_codes.ERR_CREDIT_EXHAUSTED, "Free plan credit exhausted. Upgrade to Scale to continue.", req_id),
            else => common.internalOperationError(res, "Failed to compile harness profile", req_id),
        }
        log.err("harness.compile_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        return;
    };

    log.info("harness.compiled workspace_id={s} agent_id={s} is_valid={}", .{ workspace_id, out.agent_id, out.is_valid });

    common.writeJson(res, .ok, .{
        .compile_job_id = out.compile_job_id,
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .is_valid = out.is_valid,
        .validation_report_json = out.validation_report_json,
        .request_id = req_id,
    });
}

pub fn handleActivateHarness(ctx: *Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.ActivateInput;
    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    log.debug("harness.activate workspace_id={s}", .{workspace_id});

    const out = harness_handlers.activateProfile(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidIdShape => common.errorResponse(res, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid config_version_id format", req_id),
            error.ProfileNotFound => common.errorResponse(res, .not_found, error_codes.ERR_PROFILE_NOT_FOUND, "Profile version not found", req_id),
            error.ProfileInvalid => common.errorResponse(res, .conflict, error_codes.ERR_PROFILE_INVALID, "Invalid profile cannot be activated", req_id),
            error.EntitlementMissing => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied", req_id);
            },
            error.EntitlementProfileLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded", req_id);
            },
            error.EntitlementStageLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded", req_id);
            },
            error.EntitlementSkillNotAllowed => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, req_id);
                common.errorResponse(res, .forbidden, error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills", req_id);
            },
            error.CreditExhausted => common.errorResponse(res, .forbidden, error_codes.ERR_CREDIT_EXHAUSTED, "Free plan credit exhausted. Upgrade to Scale to continue.", req_id),
            else => common.internalOperationError(res, "Failed to activate profile", req_id),
        }
        log.err("harness.activate_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        return;
    };

    log.info("harness.activated workspace_id={s} agent_id={s} config_version_id={s}", .{ workspace_id, out.agent_id, out.config_version_id });

    posthog_events.trackProfileActivated(
        ctx.posthog,
        posthog_events.distinctIdOrSystem(principal.user_id orelse ""),
        workspace_id,
        out.agent_id,
        out.config_version_id,
        out.run_snapshot_config_version,
        req_id,
    );

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .run_snapshot_config_version = out.run_snapshot_config_version,
        .activated_by = out.activated_by,
        .activated_at = out.activated_at,
        .request_id = req_id,
    });
}

pub fn handleGetHarnessActive(ctx: *Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, principal.user_id orelse API_ACTOR, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    log.debug("harness.get_active workspace_id={s}", .{workspace_id});

    const out = harness_handlers.getActiveProfile(conn, alloc, workspace_id) catch {
        log.err("harness.get_active_fail error_code=UZ-INTERNAL-003 workspace_id={s}", .{workspace_id});
        common.internalOperationError(res, "Failed to resolve active profile", req_id);
        return;
    };
    defer alloc.free(out.profile_json);

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, out.profile_json, .{}) catch {
        common.internalOperationError(res, "Failed to render profile JSON", req_id);
        return;
    };
    defer parsed.deinit();
    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .source = out.source,
        .agent_id = out.agent_id,
        .config_version_id = out.config_version_id,
        .run_snapshot_config_version = out.run_snapshot_config_version,
        .active_at = out.active_at,
        .profile = parsed.value,
        .request_id = req_id,
    });
}
