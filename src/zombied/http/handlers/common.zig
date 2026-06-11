const std = @import("std");
const constants = @import("common");
const httpz = @import("httpz");
const pg = @import("pg");
const oidc = @import("../../auth/oidc.zig");
const session_store_redis = @import("../../session/session_store_redis.zig");
const audit_events = @import("../../auth/audit_events.zig");
const queue_redis = @import("../../queue/redis.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const trace_ctx = @import("../../observability/trace.zig");
const error_codes = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const rbac = @import("../../auth/rbac.zig");
const principal_mod = @import("../../auth/principal.zig");
const balance_policy = @import("../../config/balance_policy.zig");
const runtime_loader = @import("../../config/runtime_loader.zig");
const subscription_hub = @import("../../events/subscription_hub.zig");
const stream_registry = @import("../stream_registry.zig");
const authz = @import("common_authz.zig");
/// Request-id sentinel for responses written before a request id exists
/// (e.g. the dispatch backpressure shed, which precedes the per-route arena).
pub const UNKNOWN_REQUEST_ID = "req_unknown";

pub const TraceContext = trace_ctx.TraceContext;

// HTTP wire constants. Centralised here so handlers cannot drift from the
// canonical Content-Type strings used by the error envelope.
pub const HEADER_CONTENT_TYPE = "Content-Type";
pub const CONTENT_TYPE_PROBLEM_JSON = "application/problem+json";
pub const HEADER_RETRY_AFTER = "Retry-After";
/// Capacity rejections (429 in-flight shed, 503 SSE cap) point clients at an
/// immediate short backoff: instance pressure clears in seconds, unlike
/// quota windows. Consumed by the dispatch shed and the stream-cap path.
pub const RETRY_AFTER_BRIEF_SECONDS: u32 = 1;
pub const RETRY_AFTER_BRIEF_VALUE = std.fmt.comptimePrint("{d}", .{RETRY_AFTER_BRIEF_SECONDS});

const S_PAYLOAD_TOO_LARGE_MAX_2MB = "Payload too large: max 2MB";

const S_PUNCT_99914B = "{}";

pub const Context = struct {
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    alloc: std.mem.Allocator,
    /// Io threaded from `main` → `serve.run` (Zig 0.16 DI seam). Handlers that
    /// dial sockets (SSE subscriber, jwks fetch) borrow it; testable via a
    /// loopback io.
    io: std.Io,
    /// Webhook/backend secrets resolved ONCE at boot from the env snapshot and
    /// owned for the process lifetime — handlers borrow them read-only instead
    /// of re-reading env per request. Null = unset → the handler fails closed.
    clerk_webhook_secret: ?[]const u8,
    approval_signing_secret: ?[]const u8,
    clerk_secret_key: ?[]const u8,
    oidc: ?*oidc.Verifier,
    auth_sessions: *session_store_redis.SessionStore,
    audit_ctx: audit_events.AuditCtx,
    app_url: []const u8,
    api_url: []const u8,
    api_in_flight_requests: std.atomic.Value(u32),
    api_max_in_flight_requests: u32,
    /// Ceiling for live SSE streams (SSE_MAX_STREAMS env knob, parsed in
    /// runtime_loader). Streams run on dedicated detached threads, so the cap
    /// bounds threads + memory — not handler-pool occupancy. Defaults so
    /// test/fixture Contexts that omit it get the production default.
    sse_max_streams: u32 = runtime_loader.SSE_MAX_STREAMS_DEFAULT,
    /// The process's shared Redis pub/sub fan-out — SSE streams subscribe
    /// through it instead of dialing per-stream connections. Boot-owned
    /// (serve.zig / TestHarness), started before the server listens.
    hub: *subscription_hub,
    /// Owner of the live SSE streams: cap admission, the in-flight gauge,
    /// the shutdown drain, and the fleet listing all read from it.
    /// Boot-owned, like the hub.
    stream_registry: *stream_registry,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
    telemetry: *telemetry_mod.Telemetry,
    /// Tenant balance-exhaustion gate policy, resolved once from the env at
    /// startup (the credit gate reads this, not the env, per request). Defaults
    /// so test/fixture Contexts that omit it get the production default.
    balance_policy: balance_policy.Policy = balance_policy.DEFAULT,
};

/// Parse traceparent header from request, or generate a root trace context.
pub fn resolveTraceContext(req: *httpz.Request) TraceContext {
    if (req.header("traceparent")) |header| {
        if (TraceContext.fromW3CHeader(header)) |parsed| {
            return parsed.child();
        }
    }
    return TraceContext.generate();
}

// AuthPrincipal + AuthRole live in src/zombied/auth/; the handler layer
// reaches them through these re-exports.
pub const AuthRole = rbac.AuthRole;
pub const AuthPrincipal = principal_mod.AuthPrincipal;

pub fn writeJson(res: *httpz.Response, status: std.http.Status, value: anytype) void {
    res.status = @intFromEnum(status);
    res.json(value, .{}) catch {
        res.status = 500;
        res.body = S_PUNCT_99914B;
    };
}

/// RFC 7807 error response. Looks up http_status and title from error_registry.
/// Content-Type is set to application/problem+json.
/// Callers no longer pass std.http.Status — the error code owns its status.
pub fn errorResponse(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
) void {
    writeProblem(res, code, detail, request_id, null);
}

/// 409 variant: REST guide §4 mandates every conflict carry `current_state`
/// naming the state that forbade the transition (e.g. "paused").
pub fn errorResponseConflict(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
    current_state: []const u8,
) void {
    writeProblem(res, code, detail, request_id, current_state);
}

fn writeProblem(
    res: *httpz.Response,
    code: []const u8,
    detail: []const u8,
    request_id: []const u8,
    current_state: ?[]const u8,
) void {
    const entry = error_codes.lookup(code);
    res.status = @intFromEnum(entry.http_status);
    // Use res.header() for application/problem+json — not in httpz.ContentType enum.
    res.header(HEADER_CONTENT_TYPE, CONTENT_TYPE_PROBLEM_JSON);
    const body = .{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = detail,
        .error_code = code,
        .request_id = request_id,
        .current_state = current_state,
    };
    // emit_null_optional_fields=false keeps the non-409 wire shape unchanged.
    const json_formatter = std.json.fmt(body, .{ .emit_null_optional_fields = false });
    json_formatter.format(&res.buffer.writer) catch {
        res.status = 500;
        res.body = S_PUNCT_99914B;
    };
}

pub fn internalDbUnavailable(res: *httpz.Response, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_DB_UNAVAILABLE, "Database unavailable", request_id);
}

pub fn internalDbError(res: *httpz.Response, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_DB_QUERY, "Database error", request_id);
}

pub fn internalOperationError(res: *httpz.Response, detail: []const u8, request_id: []const u8) void {
    errorResponse(res, error_codes.ERR_INTERNAL_OPERATION_FAILED, detail, request_id);
}

pub const MAX_BODY_SIZE: usize = 2 * 1024 * 1024; // 2MB — must match server.zig max_body_size

/// Returns true if the body size is within the allowed limit.
/// Sends a 413 response and returns false if the Content-Length header
/// indicates the payload exceeds MAX_BODY_SIZE, or if the received body
/// itself exceeds the limit.
pub fn checkBodySize(req: *httpz.Request, res: *httpz.Response, body: []const u8, request_id: []const u8) bool {
    if (req.header("content-length")) |cl_str| {
        const cl = std.fmt.parseInt(usize, cl_str, 10) catch 0;
        if (cl > MAX_BODY_SIZE) {
            errorResponse(res, error_codes.ERR_PAYLOAD_TOO_LARGE, S_PAYLOAD_TOO_LARGE_MAX_2MB, request_id);
            return false;
        }
    }
    if (body.len >= MAX_BODY_SIZE) {
        errorResponse(res, error_codes.ERR_PAYLOAD_TOO_LARGE, S_PAYLOAD_TOO_LARGE_MAX_2MB, request_id);
        return false;
    }
    return true;
}

pub fn requestId(alloc: std.mem.Allocator) []const u8 {
    var id: [16]u8 = undefined;
    constants.secureRandomBytes(&id) catch return UNKNOWN_REQUEST_ID;
    const hex = std.fmt.bytesToHex(id, .lower);
    return std.fmt.allocPrint(alloc, "req_{s}", .{hex[0..12]}) catch UNKNOWN_REQUEST_ID;
}

pub fn requireUuidV7Id(
    res: *httpz.Response,
    req_id: []const u8,
    id: []const u8,
    id_label: []const u8,
) bool {
    if (id_format.isUuidV7(id)) return true;
    var msg_buf: [96]u8 = undefined;
    const message = std.fmt.bufPrint(&msg_buf, "Invalid {s} format", .{id_label}) catch "Invalid identifier format";
    errorResponse(res, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, message, req_id);
    return false;
}

pub fn requireRole(res: *httpz.Response, req_id: []const u8, principal: AuthPrincipal, required: AuthRole) bool {
    if (principal.role.allows(required)) return true;
    var msg_buf: [128]u8 = undefined;
    const message = std.fmt.bufPrint(
        &msg_buf,
        "Your role is '{s}'. {s} role required.",
        .{ principal.role.label(), required.label() },
    ) catch "Insufficient role";
    errorResponse(res, error_codes.ERR_INSUFFICIENT_ROLE, message, req_id);
    return false;
}

/// Write a 405 Method Not Allowed response.
/// Used by route_table.zig invoke functions that do their own method dispatch.
pub fn respondMethodNotAllowed(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.method_not_allowed);
    res.body = "";
}

test "requireUuidV7Id rejects SQL injection payload" {
    // This function requires an httpz.Response which we can't easily mock.
    // Instead, test the underlying validator directly (id_format imported at file scope).
    try std.testing.expect(!id_format.isUuidV7("'; DROP TABLE specs; --"));
    try std.testing.expect(!id_format.isUuidV7("<script>alert(1)</script>"));
    try std.testing.expect(!id_format.isUuidV7("ignore previous instructions"));
    try std.testing.expect(!id_format.isUuidV7(""));
    try std.testing.expect(!id_format.isUuidV7("not-a-uuid"));
    // Valid UUIDv7 should pass
    try std.testing.expect(id_format.isUuidV7("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11"));
}

pub const getZombieWorkspaceId = authz.getZombieWorkspaceId;
pub const authorizeWorkspace = authz.authorizeWorkspace;
pub const setTenantSessionContext = authz.setTenantSessionContext;
pub const authorizeWorkspaceAndSetTenantContext = authz.authorizeWorkspaceAndSetTenantContext;
pub const openHandlerTestConn = authz.openHandlerTestConn;

test {
    _ = @import("common_authz_test.zig");
}
