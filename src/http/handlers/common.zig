const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const oidc = @import("../../auth/oidc.zig");
const auth_sessions = @import("../../auth/sessions.zig");
const queue_redis = @import("../../queue/redis.zig");
const worker = @import("../../pipeline/worker.zig");
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const db = @import("../../db/pool.zig");

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
};

pub const AuthMode = enum {
    api_key,
    jwt_oidc,
};

pub const AuthPrincipal = struct {
    mode: AuthMode,
    tenant_id: ?[]const u8 = null,
    workspace_scope_id: ?[]const u8 = null,
};

pub const AuthError = error{
    Unauthorized,
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
        .@"error" = .{ .code = code, .message = message },
        .request_id = request_id,
    });
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

fn authenticateApiKey(
    alloc: std.mem.Allocator,
    r: zap.Request,
    ctx: *Context,
) AuthError!AuthPrincipal {
    const provided = parseBearerToken(r) orelse return AuthError.Unauthorized;

    // Backward compatible behavior for non-empty env key list until DB keys are provisioned.
    var it = std.mem.tokenizeScalar(u8, ctx.api_keys, ',');
    while (it.next()) |candidate_raw| {
        const candidate = std.mem.trim(u8, candidate_raw, " \t");
        if (candidate.len == 0) continue;
        if (!std.mem.eql(u8, provided, candidate)) continue;
        return .{
            .mode = .api_key,
            .tenant_id = alloc.dupe(u8, "github_app") catch return AuthError.AuthServiceUnavailable,
            .workspace_scope_id = null,
        };
    }

    return AuthError.Unauthorized;
}

pub fn authenticate(alloc: std.mem.Allocator, r: zap.Request, ctx: *Context) AuthError!AuthPrincipal {
    if (ctx.oidc) |verifier| {
        const auth = r.getHeader("authorization") orelse return AuthError.Unauthorized;
        const principal = verifier.verifyAuthorization(alloc, auth) catch |err| return mapOidcVerifyError(err);
        return .{
            .mode = .jwt_oidc,
            .tenant_id = principal.tenant_id,
            .workspace_scope_id = principal.workspace_id,
        };
    }

    return authenticateApiKey(alloc, r, ctx);
}

pub fn writeAuthError(r: zap.Request, req_id: []const u8, err: AuthError) void {
    switch (err) {
        AuthError.TokenExpired => errorResponse(r, .unauthorized, "token_expired", "token expired", req_id),
        AuthError.Unauthorized => errorResponse(r, .unauthorized, "UNAUTHORIZED", "Invalid or missing token", req_id),
        AuthError.AuthServiceUnavailable => errorResponse(r, .service_unavailable, "AUTH_UNAVAILABLE", "Authentication service unavailable", req_id),
    }
}

pub fn mapOidcVerifyError(err: anyerror) AuthError {
    return switch (err) {
        error.TokenExpired => AuthError.TokenExpired,
        error.JwksFetchFailed, error.JwksParseFailed => AuthError.AuthServiceUnavailable,
        else => AuthError.Unauthorized,
    };
}

pub fn authorizeWorkspace(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    const tenant_id = principal.tenant_id orelse return false;

    var q = conn.query(
        "SELECT 1 FROM workspaces WHERE workspace_id = $1 AND tenant_id = $2",
        .{ workspace_id, tenant_id },
    ) catch return false;
    defer q.deinit();
    if ((q.next() catch null) == null) return false;

    if (principal.workspace_scope_id) |scoped_workspace_id| {
        if (!std.mem.eql(u8, scoped_workspace_id, workspace_id)) return false;
    }
    return true;
}

pub fn setTenantSessionContext(conn: *pg.Conn, tenant_id: []const u8) bool {
    var q = conn.query("SELECT set_config('app.current_tenant_id', $1, true)", .{tenant_id}) catch return false;
    q.deinit();
    return true;
}

pub fn authorizeWorkspaceAndSetTenantContext(conn: *pg.Conn, principal: AuthPrincipal, workspace_id: []const u8) bool {
    const tenant_id = principal.tenant_id orelse return false;
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
    _ = conn.query(
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
    _ = conn.query(
        "UPDATE runs SET state = $1, updated_at = $2 WHERE run_id = $3",
        .{ previous_state, std.time.milliTimestamp(), run_id },
    ) catch {};
    _ = conn.query(
        "DELETE FROM run_transitions WHERE run_id = $1 AND reason_code = 'MANUAL_RETRY' AND ts = $2",
        .{ run_id, transition_ts },
    ) catch {};
}

pub fn openHandlerTestConn(alloc: std.mem.Allocator) !?struct { pool: *db.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try db.parseUrl(arena.allocator(), url);
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

test "integration: api key workspace scoping blocks cross-workspace access" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ('ws_a', 'tenant_a'), ('ws_b', 'tenant_a')",
            .{},
        );
        q.deinit();
    }

    const principal = AuthPrincipal{
        .mode = .api_key,
        .tenant_id = "tenant_a",
        .workspace_scope_id = "ws_a",
    };
    try std.testing.expect(authorizeWorkspace(db_ctx.conn, principal, "ws_a"));
    try std.testing.expect(!authorizeWorkspace(db_ctx.conn, principal, "ws_b"));
}

test "integration: clerk workspace claim scoping blocks cross-workspace access" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE workspaces (
            \\  workspace_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO workspaces (workspace_id, tenant_id) VALUES ('ws_a', 'tenant_a'), ('ws_b', 'tenant_a')",
            .{},
        );
        q.deinit();
    }

    const principal = AuthPrincipal{
        .mode = .jwt_oidc,
        .tenant_id = "tenant_a",
        .workspace_scope_id = "ws_a",
    };
    try std.testing.expect(authorizeWorkspace(db_ctx.conn, principal, "ws_a"));
    try std.testing.expect(!authorizeWorkspace(db_ctx.conn, principal, "ws_b"));
}

test "integration: tenant context helper writes app.current_tenant_id" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "tenant_ctx"));
    var q = try db_ctx.conn.query(
        "SELECT current_setting('app.current_tenant_id', true)",
        .{},
    );
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const current_tenant = try row.get(?[]const u8, 0);
    try std.testing.expect(current_tenant != null);
    try std.testing.expectEqualStrings("tenant_ctx", current_tenant.?);
}

test "integration: RLS policy enforces tenant session isolation" {
    const db_ctx = (try openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE rls_probe (
            \\  tenant_id TEXT NOT NULL,
            \\  value TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query("ALTER TABLE rls_probe ENABLE ROW LEVEL SECURITY", .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query("ALTER TABLE rls_probe FORCE ROW LEVEL SECURITY", .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE POLICY rls_probe_select_tenant ON rls_probe
            \\FOR SELECT USING (tenant_id = current_setting('app.current_tenant_id', true))
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE POLICY rls_probe_insert_tenant ON rls_probe
            \\FOR INSERT WITH CHECK (tenant_id = current_setting('app.current_tenant_id', true))
        , .{});
        q.deinit();
    }

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "tenant_a"));
    {
        var q = try db_ctx.conn.query("INSERT INTO rls_probe (tenant_id, value) VALUES ('tenant_a', 'a1')", .{});
        q.deinit();
    }
    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "tenant_b"));
    {
        var q = try db_ctx.conn.query("INSERT INTO rls_probe (tenant_id, value) VALUES ('tenant_b', 'b1')", .{});
        q.deinit();
    }

    try std.testing.expect(setTenantSessionContext(db_ctx.conn, "tenant_a"));
    var count_q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM rls_probe", .{});
    defer count_q.deinit();
    const row = (try count_q.next()) orelse return error.TestUnexpectedResult;
    const visible_rows = try row.get(i64, 0);
    try std.testing.expectEqual(@as(i64, 1), visible_rows);
}
