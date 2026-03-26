const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const posthog = @import("posthog");
const oidc = @import("../../auth/oidc.zig");
const auth_sessions = @import("../../auth/sessions.zig");
const queue_redis = @import("../../queue/redis.zig");
const worker = @import("../../pipeline/worker.zig");
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const posthog_events = @import("../../observability/posthog_events.zig");
const trace_ctx = @import("../../observability/trace.zig");
const db = @import("../../db/pool.zig");
const error_codes = @import("../../errors/codes.zig");
const id_format = @import("../../types/id_format.zig");
const rbac = @import("../rbac.zig");

pub const TraceContext = trace_ctx.TraceContext;

pub const Context = struct {
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    alloc: std.mem.Allocator,
    api_keys: []const u8,
    oidc: ?*oidc.Verifier,
    auth_sessions: *auth_sessions.SessionStore,
    app_url: []const u8,
    worker_state: *const worker.WorkerState,
    api_in_flight_requests: std.atomic.Value(u32),
    api_max_in_flight_requests: u32,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
    posthog: ?*posthog.PostHogClient = null,
};

/// Parse traceparent header from request, or generate a root trace context.
pub fn resolveTraceContext(r: zap.Request) TraceContext {
    if (r.getHeader("traceparent")) |header| {
        if (TraceContext.fromW3CHeader(header)) |parsed| {
            return parsed.child();
        }
    }
    return TraceContext.generate();
}

/// Format trace_id from TraceContext as a slice for passing to RunContext.
pub fn traceIdFromContext(tctx: *const TraceContext) []const u8 {
    return tctx.traceIdSlice();
}

pub const AuthMode = enum {
    api_key,
    jwt_oidc,
};
pub const AuthRole = rbac.AuthRole;

pub const AuthPrincipal = struct {
    mode: AuthMode,
    role: AuthRole = .user,
    user_id: ?[]const u8 = null,
    tenant_id: ?[]const u8 = null,
    workspace_scope_id: ?[]const u8 = null,
};

pub const AuthError = error{
    Unauthorized,
    UnsupportedRole,
    TokenExpired,
    AuthServiceUnavailable,
};

pub fn writeJson(r: zap.Request, status: zap.http.StatusCode, value: anytype) void {
    var buf: [64 * 1024]u8 = undefined;
    var fba = std.heap.FixedBufferAllocator.init(&buf);
    const json = std.json.Stringify.valueAlloc(fba.allocator(), value, .{}) catch {
        r.setStatus(.internal_server_error);
        r.sendBody("{}") catch |err| obs_log.logWarnErr(.http, err, "writeJson fallback send failed", .{});
        return;
    };
    r.setStatus(status);
    r.setContentType(.JSON) catch |err| obs_log.logWarnErr(.http, err, "setContentType failed", .{});
    r.sendBody(json) catch |err| obs_log.logWarnErr(.http, err, "sendBody failed", .{});
}

pub fn errorResponse(
    r: zap.Request,
    status: zap.http.StatusCode,
    code: []const u8,
    message: []const u8,
    request_id: []const u8,
) void {
    writeJson(r, status, .{
        .@"error" = .{
            .code = code,
            .message = message,
        },
        .request_id = request_id,
    });
}

pub fn internalDbUnavailable(r: zap.Request, request_id: []const u8) void {
    errorResponse(r, .service_unavailable, error_codes.ERR_INTERNAL_DB_UNAVAILABLE, "Database unavailable", request_id);
}

pub fn internalDbError(r: zap.Request, request_id: []const u8) void {
    errorResponse(r, .internal_server_error, error_codes.ERR_INTERNAL_DB_QUERY, "Database error", request_id);
}

pub fn internalOperationError(r: zap.Request, message: []const u8, request_id: []const u8) void {
    errorResponse(r, .internal_server_error, error_codes.ERR_INTERNAL_OPERATION_FAILED, message, request_id);
}

pub const MAX_BODY_SIZE: usize = 2 * 1024 * 1024; // 2MB — must match server.zig max_body_size

/// Returns true if the body size is within the allowed limit.
/// Sends a 413 response and returns false if the Content-Length header
/// indicates the payload exceeds MAX_BODY_SIZE, or if the received body
/// itself exceeds the limit after facil.io truncation.
pub fn checkBodySize(r: zap.Request, body: []const u8, request_id: []const u8) bool {
    if (r.getHeader("content-length")) |cl_str| {
        const cl = std.fmt.parseInt(usize, cl_str, 10) catch 0;
        if (cl > MAX_BODY_SIZE) {
            errorResponse(r, .content_too_large, error_codes.ERR_PAYLOAD_TOO_LARGE, "Payload too large: max 2MB", request_id);
            return false;
        }
    }
    if (body.len >= MAX_BODY_SIZE) {
        errorResponse(r, .content_too_large, error_codes.ERR_PAYLOAD_TOO_LARGE, "Payload too large: max 2MB", request_id);
        return false;
    }
    return true;
}

pub fn requestId(alloc: std.mem.Allocator) []const u8 {
    var id: [16]u8 = undefined;
    std.crypto.random.bytes(&id);
    const hex = std.fmt.bytesToHex(id, .lower);
    return std.fmt.allocPrint(alloc, "req_{s}", .{hex[0..12]}) catch "req_unknown";
}

fn parseBearerToken(r: zap.Request) ?[]const u8 {
    const auth = r.getHeader("authorization") orelse return null;
    const prefix = "Bearer ";
    if (!std.mem.startsWith(u8, auth, prefix)) return null;
    const provided = auth[prefix.len..];
    if (std.mem.trim(u8, provided, " \t\r\n").len == 0) return null;
    return provided;
}

fn matchApiKey(provided: []const u8, configured_keys: []const u8) bool {
    var it = std.mem.tokenizeScalar(u8, configured_keys, ',');
    while (it.next()) |candidate_raw| {
        const candidate = std.mem.trim(u8, candidate_raw, " \t");
        if (candidate.len == 0) continue;
        if (std.mem.eql(u8, provided, candidate)) return true;
    }
    return false;
}

fn authenticateApiKey(provided: []const u8, ctx: *Context) AuthError!AuthPrincipal {
    if (!matchApiKey(provided, ctx.api_keys)) return AuthError.Unauthorized;
    return .{
        .mode = .api_key,
        .role = .admin,
        .user_id = null,
        .tenant_id = null,
        .workspace_scope_id = null,
    };
}

pub fn authenticate(alloc: std.mem.Allocator, r: zap.Request, ctx: *Context) AuthError!AuthPrincipal {
    const provided = parseBearerToken(r) orelse return AuthError.Unauthorized;
    if (authenticateApiKey(provided, ctx)) |principal| {
        return principal;
    } else |err| switch (err) {
        AuthError.Unauthorized => {},
        else => return err,
    }

    if (ctx.oidc) |verifier| {
        const auth = r.getHeader("authorization") orelse return AuthError.Unauthorized;
        const principal = verifier.verifyAuthorization(alloc, auth) catch |err| switch (err) {
            error.TokenExpired => return AuthError.TokenExpired,
            error.JwksFetchFailed, error.JwksParseFailed => return AuthError.AuthServiceUnavailable,
            else => return AuthError.Unauthorized,
        };
        if (principal.tenant_id) |tenant_id| {
            if (!id_format.isSupportedTenantId(tenant_id)) return AuthError.Unauthorized;
        }
        if (principal.workspace_id) |workspace_id| {
            if (!id_format.isSupportedWorkspaceId(workspace_id)) return AuthError.Unauthorized;
        }
        // SAFETY: claims.zig normalizes all role strings through rbac.parseAuthRole
        // before storing them in IdentityClaims.role, so raw is always a valid role
        // label or null. The UnsupportedRole branch guards against future claim
        // extraction paths that might skip normalization.
        const role = if (principal.role) |raw| rbac.parseAuthRole(raw) orelse {
            return AuthError.UnsupportedRole;
        } else AuthRole.user;
        return .{
            .mode = .jwt_oidc,
            .role = role,
            .user_id = principal.subject,
            .tenant_id = principal.tenant_id,
            .workspace_scope_id = principal.workspace_id,
        };
    }

    return AuthError.Unauthorized;
}

test "matchApiKey accepts configured key in rotation list" {
    try std.testing.expect(matchApiKey("key-b", "key-a, key-b, key-c"));
}

test "matchApiKey rejects empty and non-matching candidates" {
    try std.testing.expect(!matchApiKey("key-z", "key-a, , key-b"));
    try std.testing.expect(!matchApiKey("key-a", ""));
}

pub fn requireUuidV7Id(
    r: zap.Request,
    req_id: []const u8,
    id: []const u8,
    id_label: []const u8,
) bool {
    if (id_format.isUuidV7(id)) return true;
    var msg_buf: [96]u8 = undefined;
    const message = std.fmt.bufPrint(&msg_buf, "Invalid {s} format", .{id_label}) catch "Invalid identifier format";
    errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, message, req_id);
    return false;
}

pub fn writeAuthError(r: zap.Request, req_id: []const u8, err: AuthError) void {
    writeAuthErrorWithTracking(r, req_id, err, null);
}

pub fn writeAuthErrorWithTracking(r: zap.Request, req_id: []const u8, err: AuthError, ph_client: ?*posthog.PostHogClient) void {
    const reason: []const u8 = switch (err) {
        AuthError.TokenExpired => "token_expired",
        AuthError.Unauthorized => "unauthorized",
        AuthError.UnsupportedRole => "unsupported_role",
        AuthError.AuthServiceUnavailable => "auth_service_unavailable",
    };
    posthog_events.trackAuthRejected(ph_client, reason, req_id);
    switch (err) {
        AuthError.TokenExpired => errorResponse(r, .unauthorized, error_codes.ERR_TOKEN_EXPIRED, "token expired", req_id),
        AuthError.Unauthorized => errorResponse(r, .unauthorized, error_codes.ERR_UNAUTHORIZED, "Invalid or missing token", req_id),
        AuthError.UnsupportedRole => errorResponse(r, .forbidden, error_codes.ERR_UNSUPPORTED_ROLE, "Unsupported role in token", req_id),
        AuthError.AuthServiceUnavailable => errorResponse(r, .service_unavailable, error_codes.ERR_AUTH_UNAVAILABLE, "Authentication service unavailable", req_id),
    }
}

pub fn requireRole(r: zap.Request, req_id: []const u8, principal: AuthPrincipal, required: AuthRole) bool {
    if (principal.role.allows(required)) return true;
    var msg_buf: [64]u8 = undefined;
    const message = std.fmt.bufPrint(&msg_buf, "{s} role required", .{required.label()}) catch "Insufficient role";
    errorResponse(r, .forbidden, error_codes.ERR_INSUFFICIENT_ROLE, message, req_id);
    return false;
}

pub fn mapOidcVerifyError(err: anyerror) AuthError {
    return switch (err) {
        error.TokenExpired => AuthError.TokenExpired,
        error.JwksFetchFailed, error.JwksParseFailed => AuthError.AuthServiceUnavailable,
        else => AuthError.Unauthorized,
    };
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    var q = blk: {
        if (principal.tenant_id) |tenant_id| {
            break :blk conn.query(
                "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND tenant_id = $2",
                .{ workspace_id, tenant_id },
            ) catch return false;
        }
        break :blk conn.query(
            "SELECT 1 FROM workspaces WHERE workspace_id = $1",
            .{workspace_id},
        ) catch return false;
    };
    defer q.deinit();
    const row = (q.next() catch return false) orelse return false;
    _ = row;
    q.drain() catch return false;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return false;
    }
    return true;
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    // is_local=false: session-level setting so subsequent statements on the same
    // connection see the value. In production each request handler always calls
    // this before touching tenant-scoped tables, so there is no cross-tenant leak.
    _ = conn.exec("SELECT set_config('app.current_tenant_id', $1, false)", .{tenant_id}) catch return false;
    return true;
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    const tenant_id = principal.tenant_id orelse blk: {
        var lookup = conn.query("SELECT tenant_id FROM workspaces WHERE workspace_id = $1", .{workspace_id}) catch return false;
        defer lookup.deinit();
        const row = (lookup.next() catch return false) orelse return false;
        const tid = row.get([]u8, 0) catch return false;
        lookup.drain() catch return false;
        break :blk tid;
    };
    if (!setTenantSessionContext(conn, tenant_id)) return false;
    return authorizeWorkspace(conn, principal, workspace_id);
}

pub fn beginApiRequest(ctx: *Context) bool {
    const prev = ctx.api_in_flight_requests.fetchAdd(1, .acq_rel);
    if (prev >= ctx.api_max_in_flight_requests) {
        const reverted = ctx.api_in_flight_requests.fetchSub(1, .acq_rel);
        std.debug.assert(reverted > 0);
        metrics.incApiBackpressureRejections();
        return false;
    }

    metrics.setApiInFlightRequests(ctx.api_in_flight_requests.load(.acquire));
    return true;
}

pub fn endApiRequest(ctx: *Context) void {
    const prev = ctx.api_in_flight_requests.fetchSub(1, .acq_rel);
    std.debug.assert(prev > 0);
    metrics.setApiInFlightRequests(ctx.api_in_flight_requests.load(.acquire));
}

pub fn compensateStartRunQueueFailure(conn: *pg.Conn, run_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM runs WHERE run_id = $1 AND state = 'SPEC_QUEUED'",
        .{run_id},
    ) catch {};
}

pub fn compensateRetryQueueFailure(
    conn: *pg.Conn,
    run_id: []const u8,
    previous_state: []const u8,
    transition_ts: i64,
) void {
    _ = conn.exec(
        "UPDATE runs SET state = $1, updated_at = $2 WHERE run_id = $3",
        .{ previous_state, std.time.milliTimestamp(), run_id },
    ) catch {};
    _ = conn.exec(
        "DELETE FROM run_transitions WHERE run_id = $1 AND reason_code = 'MANUAL_RETRY' AND ts = $2",
        .{ run_id, transition_ts },
    ) catch {};
}

pub fn openHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    // check-pg-drain: ok — no conn.query() here; checker misattributes test-block queries
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);
    // Use page_allocator for opts strings so they outlive the pool. pg.Pool stores
    // shallow references to connect.host/auth strings — if the arena they come from
    // is freed first, pool.release() crashes when it calls newConnection() on a
    // non-idle conn (e.g. after an internal query failure in the test body).
    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}

test "mapOidcVerifyError maps expired token to token_expired response path" {
    try std.testing.expectEqual(AuthError.TokenExpired, mapOidcVerifyError(oidc.VerifyError.TokenExpired));
}

test "mapOidcVerifyError maps jwks failures to auth unavailable" {
    try std.testing.expectEqual(AuthError.AuthServiceUnavailable, mapOidcVerifyError(oidc.VerifyError.JwksFetchFailed));
    try std.testing.expectEqual(AuthError.AuthServiceUnavailable, mapOidcVerifyError(oidc.VerifyError.JwksParseFailed));
}

test "mapOidcVerifyError maps signature failures to unauthorized" {
    try std.testing.expectEqual(AuthError.Unauthorized, mapOidcVerifyError(oidc.VerifyError.SignatureInvalid));
}

test "integration: oidc workspace scoping blocks cross-workspace access" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01'), ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01')",
        .{},
    );

    const principal = AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01",
        .workspace_scope_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
    };
    try std.testing.expect(authorizeWorkspace(db_ctx.conn, principal, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(!authorizeWorkspace(db_ctx.conn, principal, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12"));
}

test "integration: clerk workspace claim scoping blocks cross-workspace access" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE workspaces (
        \\  workspace_id UUID PRIMARY KEY,
        \\  tenant_id UUID NOT NULL
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01'), ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12', '0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01')",
        .{},
    );

    const principal = AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01",
        .workspace_scope_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
    };
    try std.testing.expect(authorizeWorkspace(db_ctx.conn, principal, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
    try std.testing.expect(!authorizeWorkspace(db_ctx.conn, principal, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f12"));
}

test "integration: tenant context helper writes app.current_tenant_id" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21"));
    var q = try db_ctx.conn.query(
        "SELECT current_setting('app.current_tenant_id', true)",
        .{},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const current_tenant = try row.get(?[]const u8, 0);
    try std.testing.expect(current_tenant != null);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f21", current_tenant.?);
}

test "integration: RLS policy enforces tenant session isolation" {
    // NOTE: This test requires a non-superuser DB connection. The POSTGRES_USER
    // docker image user is a superuser, and superusers bypass RLS even with
    // FORCE ROW LEVEL SECURITY. Set HANDLER_DB_TEST_NONSUPERUSER=1 when running
    // against a non-superuser test role to enable this assertion.
    if (!std.process.hasEnvVarConstant("HANDLER_DB_TEST_NONSUPERUSER")) return error.SkipZigTest;
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE rls_probe (
        \\  tenant_id UUID NOT NULL,
        \\  value TEXT NOT NULL
        \\)
    , .{});
    _ = try db_ctx.conn.exec("ALTER TABLE rls_probe ENABLE ROW LEVEL SECURITY", .{});
    _ = try db_ctx.conn.exec("ALTER TABLE rls_probe FORCE ROW LEVEL SECURITY", .{});
    _ = try db_ctx.conn.exec(
        \\CREATE POLICY rls_probe_select_tenant ON rls_probe
        \\FOR SELECT USING (tenant_id::text = current_setting('app.current_tenant_id', true))
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE POLICY rls_probe_insert_tenant ON rls_probe
        \\FOR INSERT WITH CHECK (tenant_id::text = current_setting('app.current_tenant_id', true))
    , .{});

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31"));
    _ = try db_ctx.conn.exec("INSERT INTO rls_probe (tenant_id, value) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31', 'a1')", .{});
    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f32"));
    _ = try db_ctx.conn.exec("INSERT INTO rls_probe (tenant_id, value) VALUES ('0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f32', 'b1')", .{});

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f31"));
    var count_q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM rls_probe", .{});
    defer count_q.deinit();
    const row = (try count_q.next()) orelse return error.TestUnexpectedResult;
    const visible_rows = try row.get(i64, 0);
    try std.testing.expectEqual(@as(i64, 1), visible_rows);
}
