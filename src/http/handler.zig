//! HTTP request handlers for all control-plane endpoints.
//! Uses Zap's request/response API. All responses follow the M1_002 error contract.

const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const secrets = @import("../secrets/crypto.zig");
const metrics = @import("../observability/metrics.zig");
const obs_log = @import("../observability/logging.zig");
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

    const Req = harness_handlers.PutSourceInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);
    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.putSource(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Invalid harness source payload", req_id),
            error.WorkspaceNotFound => common.errorResponse(r, .not_found, "WORKSPACE_NOT_FOUND", "Workspace not found", req_id),
            else => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to store harness source", req_id),
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

    const Req = harness_handlers.CompileInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.compileProfile(conn, alloc, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.ProfileNotFound => common.errorResponse(r, .not_found, "PROFILE_NOT_FOUND", "No harness profile source found for workspace", req_id),
            error.CompileFailed => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Harness compile failed", req_id),
            else => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to compile harness profile", req_id),
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

    const Req = harness_handlers.ActivateInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.activateProfile(conn, workspace_id, parsed.value) catch |err| {
        switch (err) {
            error.ProfileNotFound => common.errorResponse(r, .not_found, "PROFILE_NOT_FOUND", "Profile version not found", req_id),
            error.ProfileInvalid => common.errorResponse(r, .conflict, "PROFILE_INVALID", "Invalid profile cannot be activated", req_id),
            else => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to activate profile", req_id),
        }
        return;
    };

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .profile_version_id = out.profile_version_id,
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

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = harness_handlers.getActiveProfile(conn, alloc, workspace_id) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to resolve active profile", req_id);
        return;
    };
    defer alloc.free(out.profile_json);

    const parsed = std.json.parseFromSlice(std.json.Value, alloc, out.profile_json, .{}) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to render profile JSON", req_id);
        return;
    };
    defer parsed.deinit();
    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .source = out.source,
        .profile_version_id = out.profile_version_id,
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

    const Req = skill_secret_handlers.PutInput;
    const body = r.body orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.put(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded, parsed.value) catch |err| {
        switch (err) {
            error.InvalidRequest => common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Invalid skill secret payload", req_id),
            error.MissingMasterKey => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "ENCRYPTION_MASTER_KEY is missing", req_id),
            else => common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to store skill secret", req_id),
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

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const out = skill_secret_handlers.delete(conn, alloc, workspace_id, skill_ref_encoded, key_name_encoded) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to delete skill secret", req_id);
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
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "installation_id query param required", req_id);
        return;
    };
    defer alloc.free(installation_id);

    const workspace_id = r.getParamStr(alloc, "state") catch null orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "state query param required", req_id);
        return;
    };
    defer alloc.free(workspace_id);

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();

    {
        var t = conn.query(
            \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at)
            \\VALUES ('github_app', 'GitHub App', 'callback', $1)
            \\ON CONFLICT (tenant_id) DO NOTHING
        , .{now_ms}) catch {
            common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to upsert tenant", req_id);
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
            \\VALUES ($1, 'github_app', $2, $3, false, 1, $4, $4)
            \\ON CONFLICT (workspace_id) DO UPDATE
            \\SET repo_url = EXCLUDED.repo_url,
            \\    default_branch = EXCLUDED.default_branch,
            \\    updated_at = EXCLUDED.updated_at
        , .{ workspace_id, repo_url, default_branch, now_ms }) catch {
            common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to upsert workspace", req_id);
            return;
        };
        w.deinit();
    }

    const kek = secrets.loadKek(alloc) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "ENCRYPTION_MASTER_KEY is missing", req_id);
        return;
    };

    secrets.store(
        alloc,
        conn,
        workspace_id,
        "github_app_installation_id",
        installation_id,
        kek,
    ) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to store installation secret", req_id);
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
        common.errorResponse(r, .service_unavailable, "SESSION_LIMIT", "Too many pending sessions", req_id);
        return;
    };

    const login_url = std.fmt.allocPrint(alloc, "{s}/auth/cli?session_id={s}", .{ ctx.app_url, session_id }) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to build login URL", req_id);
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
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(struct { token: []const u8 }, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON or missing token field", req_id);
        return;
    };
    defer parsed.deinit();

    if (parsed.value.token.len == 0) {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Token must not be empty", req_id);
        return;
    }

    ctx.auth_sessions.complete(session_id, parsed.value.token) catch |err| {
        const code: []const u8 = switch (err) {
            error.SessionNotFound => "SESSION_NOT_FOUND",
            error.SessionExpired => "SESSION_EXPIRED",
            error.SessionAlreadyComplete => "SESSION_ALREADY_COMPLETE",
            else => "INTERNAL_ERROR",
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
