//! HTTP request handlers for all control-plane endpoints.
//! Uses Zap's request/response API. All responses follow the M1_002 error contract.

const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const secrets = @import("../secrets/crypto.zig");
const metrics = @import("../observability/metrics.zig");
const posthog_events = @import("../observability/posthog_events.zig");
const obs_log = @import("../observability/logging.zig");
const error_codes = @import("../errors/codes.zig");
const id_format = @import("../types/id_format.zig");
const workspace_billing = @import("../state/workspace_billing.zig");
const harness_handlers = @import("handlers/harness_control_plane.zig");
const skill_secret_handlers = @import("handlers/skill_secrets.zig");
const common = @import("handlers/common.zig");
const runs_handlers = @import("handlers/runs.zig");
const workspace_handlers = @import("handlers/workspaces.zig");
const specs_handlers = @import("handlers/specs.zig");

pub const Context = common.Context;
pub const SkillSecretRoute = skill_secret_handlers.Route;

pub const handleStartRun = runs_handlers.handleStartRun;
pub const handleGetRun = runs_handlers.handleGetRun;
pub const handleRetryRun = runs_handlers.handleRetryRun;
pub const handleCreateWorkspace = workspace_handlers.handleCreateWorkspace;
pub const handlePauseWorkspace = workspace_handlers.handlePauseWorkspace;
pub const handleSyncSpecs = workspace_handlers.handleSyncSpecs;
pub const handleUpgradeWorkspaceToScale = workspace_handlers.handleUpgradeWorkspaceToScale;
pub const handleListSpecs = specs_handlers.handleListSpecs;

pub fn parseSkillSecretRoute(path: []const u8) ?SkillSecretRoute {
    return skill_secret_handlers.parseRoute(path);
}

fn databaseHealthy(ctx: *Context) bool {
    const conn = ctx.pool.acquire() catch return false;
    defer ctx.pool.release(conn);

    var ping = conn.query("SELECT 1", .{}) catch return false;
    defer ping.deinit();

    return (ping.next() catch null) != null;
}

const QueueHealth = struct {
    queued_count: i64,
    oldest_queued_age_ms: ?i64,
};

const ReadyInputs = struct {
    db_ok: bool,
    worker_ok: bool,
    queue_dependency_ok: bool,
    queue_depth_breached: bool,
    queue_age_breached: bool,
};

fn queueHealth(ctx: *Context) ?QueueHealth {
    const conn = ctx.pool.acquire() catch return null;
    defer ctx.pool.release(conn);

    var q = conn.query(
        \\SELECT COUNT(*)::BIGINT, MIN(created_at)::BIGINT
        \\FROM runs
        \\WHERE state = 'SPEC_QUEUED'
    , .{}) catch return null;
    defer q.deinit();

    const row = (q.next() catch null) orelse return null;
    const queued_count = row.get(i64, 0) catch return null;
    const oldest_created_ms = row.get(?i64, 1) catch return null;
    const now_ms = std.time.milliTimestamp();
    const oldest_age_ms = if (oldest_created_ms) |ts| now_ms - ts else null;
    return .{
        .queued_count = queued_count,
        .oldest_queued_age_ms = oldest_age_ms,
    };
}

fn queueDependencyHealthy(ctx: *Context) bool {
    ctx.queue.readyCheck() catch |err| {
        obs_log.logWarnErr(.http, err, "readyz: redis queue dependency check failed", .{});
        return false;
    };
    return true;
}

fn readyDecision(inputs: ReadyInputs) bool {
    return inputs.db_ok and
        inputs.worker_ok and
        inputs.queue_dependency_ok and
        !inputs.queue_depth_breached and
        !inputs.queue_age_breached;
}

pub fn handleHealthz(ctx: *Context, r: zap.Request) void {
    const db_ok = databaseHealthy(ctx);
    if (!db_ok) {
        common.writeJson(r, .service_unavailable, .{
            .status = "degraded",
            .service = "zombied",
            .database = "down",
        });
        return;
    }

    common.writeJson(r, .ok, .{
        .status = "ok",
        .service = "zombied",
        .database = "up",
    });
}

pub fn handleReadyz(ctx: *Context, r: zap.Request) void {
    const db_ok = databaseHealthy(ctx);
    const worker_ok = ctx.worker_state.running.load(.acquire);
    const queue_dependency_ok = queueDependencyHealthy(ctx);
    const qh = if (db_ok) queueHealth(ctx) else null;

    var queue_depth_breached = false;
    var queue_age_breached = false;
    if (qh) |v| {
        if (ctx.ready_max_queue_depth) |limit| {
            queue_depth_breached = v.queued_count > limit;
        }
        if (ctx.ready_max_queue_age_ms) |limit| {
            if (v.oldest_queued_age_ms) |age| {
                queue_age_breached = age > limit;
            }
        }
    }

    if (!readyDecision(.{
        .db_ok = db_ok,
        .worker_ok = worker_ok,
        .queue_dependency_ok = queue_dependency_ok,
        .queue_depth_breached = queue_depth_breached,
        .queue_age_breached = queue_age_breached,
    })) {
        common.writeJson(r, .service_unavailable, .{
            .ready = false,
            .database = db_ok,
            .worker = worker_ok,
            .queue_dependency = queue_dependency_ok,
            .queue_depth = if (qh) |v| v.queued_count else null,
            .oldest_queued_age_ms = if (qh) |v| v.oldest_queued_age_ms else null,
            .queue_depth_breached = queue_depth_breached,
            .queue_age_breached = queue_age_breached,
            .queue_depth_limit = ctx.ready_max_queue_depth,
            .queue_age_limit_ms = ctx.ready_max_queue_age_ms,
        });
        return;
    }

    common.writeJson(r, .ok, .{
        .ready = true,
        .database = true,
        .worker = true,
        .queue_dependency = true,
        .queue_depth = if (qh) |v| v.queued_count else @as(i64, 0),
        .oldest_queued_age_ms = if (qh) |v| v.oldest_queued_age_ms else null,
        .queue_depth_breached = false,
        .queue_age_breached = false,
        .queue_depth_limit = ctx.ready_max_queue_depth,
        .queue_age_limit_ms = ctx.ready_max_queue_age_ms,
    });
}

pub fn handleMetrics(ctx: *Context, r: zap.Request) void {
    const qh = queueHealth(ctx);
    const body = metrics.renderPrometheus(
        ctx.alloc,
        ctx.worker_state.running.load(.acquire),
        if (qh) |v| v.queued_count else null,
        if (qh) |v| v.oldest_queued_age_ms else null,
    ) catch {
        r.setStatus(.internal_server_error);
        r.sendBody("") catch |err| obs_log.logWarnErr(.http, err, "metrics send failed", .{});
        return;
    };
    defer ctx.alloc.free(body);

    r.setStatus(.ok);
    r.setContentType(.TEXT) catch |err| obs_log.logWarnErr(.http, err, "setContentType TEXT failed", .{});
    r.sendBody(body) catch |err| obs_log.logWarnErr(.http, err, "metrics body send failed", .{});
}

pub fn handlePutHarnessSource(ctx: *Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.PutSourceInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);
    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.putSource(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Invalid harness source payload", req_id),
            error.InvalidIdShape => common.errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid profile_version_id format", req_id),
            error.WorkspaceNotFound => common.errorResponse(r, .not_found, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id),
            else => common.internalOperationError(r, "Failed to store harness source", req_id),
        }
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .profile_id = out.profile_id,
        .profile_version_id = out.profile_version_id,
        .version = out.version,
        .status = "DRAFT",
        .request_id = req_id,
    });
}

pub fn handleCompileHarness(ctx: *Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.CompileInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.compileProfile(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidIdShape => common.errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid profile_version_id format", req_id),
            error.ProfileNotFound => common.errorResponse(r, .not_found, error_codes.ERR_PROFILE_NOT_FOUND, "No harness profile source found for workspace", req_id),
            error.CompileFailed => common.internalOperationError(r, "Harness compile failed", req_id),
            error.EntitlementMissing => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied", req_id);
            },
            error.EntitlementProfileLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded", req_id);
            },
            error.EntitlementStageLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded", req_id);
            },
            error.EntitlementSkillNotAllowed => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "COMPILE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills", req_id);
            },
            else => common.internalOperationError(r, "Failed to compile harness profile", req_id),
        }
        return;
    };

    common.writeJson(r, .ok, .{
        .compile_job_id = out.compile_job_id,
        .workspace_id = workspace_id,
        .profile_id = out.profile_id,
        .profile_version_id = out.profile_version_id,
        .is_valid = out.is_valid,
        .validation_report_json = out.validation_report_json,
        .request_id = req_id,
    });
}

pub fn handleActivateHarness(ctx: *Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = harness_handlers.ActivateInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.activateProfile(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidIdShape => common.errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid profile_version_id format", req_id),
            error.ProfileNotFound => common.errorResponse(r, .not_found, error_codes.ERR_PROFILE_NOT_FOUND, "Profile version not found", req_id),
            error.ProfileInvalid => common.errorResponse(r, .conflict, error_codes.ERR_PROFILE_INVALID, "Invalid profile cannot be activated", req_id),
            error.EntitlementMissing => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_UNAVAILABLE, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_UNAVAILABLE, "Workspace entitlement missing; request denied", req_id);
            },
            error.EntitlementProfileLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_PROFILE_LIMIT, "Workspace profile limit exceeded", req_id);
            },
            error.EntitlementStageLimit => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_STAGE_LIMIT, "Plan stage limit exceeded", req_id);
            },
            error.EntitlementSkillNotAllowed => {
                posthog_events.trackEntitlementRejected(ctx.posthog, posthog_events.distinctIdOrSystem(principal.user_id orelse ""), workspace_id, "ACTIVATE", error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, req_id);
                common.errorResponse(r, .forbidden, error_codes.ERR_ENTITLEMENT_SKILL_NOT_ALLOWED, "Plan does not allow one or more profile skills", req_id);
            },
            else => common.internalOperationError(r, "Failed to activate profile", req_id),
        }
        return;
    };
    posthog_events.trackProfileActivated(
        ctx.posthog,
        posthog_events.distinctIdOrSystem(principal.user_id orelse ""),
        workspace_id,
        out.profile_id,
        out.profile_version_id,
        out.run_snapshot_version,
        req_id,
    );

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .profile_id = out.profile_id,
        .profile_version_id = out.profile_version_id,
        .run_snapshot_version = out.run_snapshot_version,
        .activated_by = out.activated_by,
        .activated_at = out.activated_at,
        .request_id = req_id,
    });
}

pub fn handleGetHarnessActive(ctx: *Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.getActiveProfile(conn, alloc, workspace_id) catch {
        common.internalOperationError(r, "Failed to resolve active profile", req_id);
        return;
    };
    defer alloc.free(out.profile_json);

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, out.profile_json, .{}) catch {
        common.internalOperationError(r, "Failed to render profile JSON", req_id);
        return;
    };
    defer parsed.deinit();
    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .source = out.source,
        .profile_id = out.profile_id,
        .profile_version_id = out.profile_version_id,
        .run_snapshot_version = out.run_snapshot_version,
        .active_at = out.active_at,
        .profile = parsed.value,
        .request_id = req_id,
    });
}

pub fn handlePutWorkspaceSkillSecret(
    ctx: *Context,
    r: zap.Request,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = skill_secret_handlers.PutInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.put(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Invalid skill secret payload", req_id),
            error.MissingMasterKey => common.internalOperationError(r, "ENCRYPTION_MASTER_KEY is missing", req_id),
            else => common.internalOperationError(r, "Failed to store skill secret", req_id),
        }
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .scope = out.scope.label(),
        .request_id = req_id,
    });
}

pub fn handleDeleteWorkspaceSkillSecret(
    ctx: *Context,
    r: zap.Request,
    workspace_id: []const u8,
    skill_ref_encoded: []const u8,
    key_name_encoded: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.delete(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded) catch {
        common.internalOperationError(r, "Failed to delete skill secret", req_id);
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .skill_ref = out.skill_ref,
        .key_name = out.key_name,
        .deleted = true,
        .request_id = req_id,
    });
}

pub fn handleGitHubCallback(ctx: *Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const installation_id = r.getParamStr(alloc, "installation_id") catch null orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "installation_id query param required", req_id);
        return;
    };
    defer alloc.free(installation_id);

    const workspace_id = r.getParamStr(alloc, "state") catch null orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "state query param required", req_id);
        return;
    };
    defer alloc.free(workspace_id);
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    const tenant_id = blk: {
        var existing = conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id}) catch {
            common.internalDbError(r, req_id);
            return;
        };
        defer existing.deinit();
        if (existing.next() catch null) |row| {
            const current_tenant = row.get([]u8, 0) catch {
                common.internalDbError(r, req_id);
                return;
            };
            break :blk alloc.dupe(u8, current_tenant) catch {
                common.internalOperationError(r, "Failed to allocate tenant id", req_id);
                return;
            };
        }
        break :blk id_format.generateTenantId(alloc) catch {
            common.internalOperationError(r, "Failed to allocate tenant id", req_id);
            return;
        };
    };
    _ = common.setTenantSessionContext(conn, tenant_id);

    {
        var t = conn.query(
            \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at)
            \\VALUES ($1, 'GitHub App', 'callback', $2)
            \\ON CONFLICT (tenant_id) DO NOTHING
        , .{ tenant_id, now_ms }) catch {
            common.internalOperationError(r, "Failed to upsert tenant", req_id);
            return;
        };
        t.deinit();
    }

    {
        const repo_url_opt = r.getParamStr(alloc, "repo_url") catch null;
        const repo_url = repo_url_opt orelse "https://github.com/unknown/unknown";
        defer if (repo_url_opt) |v| alloc.free(v);

        const default_branch_opt = r.getParamStr(alloc, "default_branch") catch null;
        const default_branch = default_branch_opt orelse "main";
        defer if (default_branch_opt) |v| alloc.free(v);

        var w = conn.query(
            \\INSERT INTO workspaces
            \\  (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
            \\VALUES ($1, $2, $3, $4, false, 1, $5, $5)
            \\ON CONFLICT (workspace_id) DO UPDATE
            \\SET tenant_id = EXCLUDED.tenant_id,
            \\    repo_url = EXCLUDED.repo_url,
            \\    default_branch = EXCLUDED.default_branch,
            \\    updated_at = EXCLUDED.updated_at
        , .{ workspace_id, tenant_id, repo_url, default_branch, now_ms }) catch {
            common.internalOperationError(r, "Failed to upsert workspace", req_id);
            return;
        };
        w.deinit();
    }

    workspace_billing.provisionFreeWorkspace(conn, alloc, workspace_id, "api") catch {
        common.internalOperationError(r, "Failed to provision free entitlement", req_id);
        return;
    };

    secrets.store(
        alloc,
        conn,
        workspace_id,
        "github_app_installation_id",
        installation_id,
        1,
    ) catch {
        common.internalOperationError(r, "Failed to store installation secret", req_id);
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .installation_id = installation_id,
        .request_id = req_id,
    });
}

pub fn handleCreateAuthSession(ctx: *Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const session_id = ctx.auth_sessions.create() catch {
        common.errorResponse(r, .service_unavailable, error_codes.ERR_SESSION_LIMIT, "Too many pending sessions", req_id);
        return;
    };

    const login_url = std.fmt.allocPrint(alloc, "{s}/auth/cli?session_id={s}", .{ ctx.app_url, session_id }) catch {
        common.internalOperationError(r, "Failed to build login URL", req_id);
        return;
    };

    common.writeJson(r, .created, .{
        .session_id = session_id,
        .login_url = login_url,
        .request_id = req_id,
    });
}

pub fn handlePollAuthSession(ctx: *Context, r: zap.Request, session_id: []const u8) void {
    const result = ctx.auth_sessions.poll(session_id);
    const status_str: []const u8 = switch (result.status) {
        .pending => "pending",
        .complete => "complete",
        .expired => "expired",
    };
    common.writeJson(r, .ok, .{ .status = status_str, .token = result.token });
}

pub fn handleCompleteAuthSession(ctx: *Context, r: zap.Request, session_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    _ = principal;

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(struct { token: []const u8 }, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing token field", req_id);
        return;
    };
    defer parsed.deinit();

    if (parsed.value.token.len == 0) {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Token must not be empty", req_id);
        return;
    }

    ctx.auth_sessions.complete(session_id, parsed.value.token) catch |err| {
        const code: []const u8 = switch (err) {
            error.SessionNotFound => error_codes.ERR_SESSION_NOT_FOUND,
            error.SessionExpired => error_codes.ERR_SESSION_EXPIRED,
            error.SessionAlreadyComplete => error_codes.ERR_SESSION_ALREADY_COMPLETE,
            else => error_codes.ERR_INTERNAL_OPERATION_FAILED,
        };
        common.errorResponse(r, .bad_request, code, @errorName(err), req_id);
        return;
    };

    common.writeJson(r, .ok, .{ .status = "complete", .request_id = req_id });
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = false,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision fails during worker restart window" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .worker_ok = false,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision passes when dependencies and guardrails are healthy" {
    try std.testing.expect(readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}
