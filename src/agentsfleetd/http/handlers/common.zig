const std = @import("std");
const constants = @import("common");
const call_deadline = @import("call_deadline");
const httpz = @import("httpz");
const pg = @import("pg");
const R2 = @import("s3");
const oidc = @import("../../auth/oidc.zig");
const session_store_redis = @import("../../session/session_store_redis.zig");
const audit_events = @import("../../auth/audit_events.zig");
const queue_redis = @import("../../queue/redis.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const trace_ctx = @import("../../observability/trace.zig");
const error_codes = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const principal_mod = @import("../../auth/principal.zig");
const balance_policy = @import("../../config/balance_policy.zig");
const runtime_loader = @import("../../config/runtime_loader.zig");
const subscription_hub = @import("../../events/subscription_hub.zig");
const fleet_set_cache = @import("../../events/fleet_set_cache.zig");
const stream_registry = @import("../stream_registry.zig");
const CredentialBroker = @import("../../credentials/broker.zig");
const QStashCredentials = @import("../../cron/Credentials.zig");
const QStashClient = @import("../../cron/QStashClient.zig");
const authz = @import("common_authz.zig");
const problem_response = @import("problem_response.zig");
/// Request-id sentinel for responses written before a request id exists
/// (e.g. the dispatch backpressure shed, which precedes the per-route arena).
pub const UNKNOWN_REQUEST_ID = "req_unknown";

pub const TraceContext = trace_ctx.TraceContext;

// HTTP wire constants. Centralised here so handlers cannot drift from the
// canonical Content-Type strings used by the error envelope.
pub const HEADER_CONTENT_TYPE = problem_response.HEADER_CONTENT_TYPE;
pub const CONTENT_TYPE_PROBLEM_JSON = problem_response.CONTENT_TYPE_PROBLEM_JSON;
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
    /// The ONE process deadline scheduler, threaded the same way as `io`. Every
    /// outbound network owner arms a guard against this single worker instead
    /// of spawning a watchdog thread per call site.
    deadline_scheduler: *call_deadline.ProcessScheduler,
    /// Webhook/backend secrets resolved ONCE at boot from the env snapshot and
    /// owned for the process lifetime — handlers borrow them read-only instead
    /// of re-reading env per request. Null = unset → the handler fails closed.
    clerk_webhook_secret: ?[]const u8,
    approval_signing_secret: ?[]const u8,
    /// GitHub App slug for the connect install URL, resolved at boot from the
    /// admin vault `github-app` entry. Null → connect degrades closed (no
    /// install URL minted) rather than pointing at a nonexistent App.
    github_app_slug: ?[]const u8 = null,
    /// Admin workspace id — the vault namespace platform connector-app secrets
    /// (`<provider>-app`) live under. One generic field for ALL connectors; the
    /// OAuth connect/callback resolve client creds on-demand from it (a rare
    /// browser flow, not a hot path). Empty → connectors fail closed. Boot-set
    /// from `serve_cfg.platform_admin_workspace_id`.
    platform_admin_workspace_id: []const u8 = "",
    qstash_credentials: ?*const QStashCredentials = null, // boot-loaded; null fails schedule surfaces closed
    qstash_exchange_override: ?QStashClient.Exchange = null, // test seam; production uses HTTP
    /// Test/dev seam: override the connector token endpoint. Null in production;
    /// integration tests point it at a loopback fake-provider.
    connector_oauth_token_endpoint_override: ?[]const u8 = null,
    /// Test/dev seam for GitHub's user-installation ownership lookup. Null in
    /// production. Integration tests point it at a loopback fake-provider.
    connector_github_api_base_override: ?[]const u8 = null,
    /// Test/dev seam: override the Slack Web API base (`https://slack.com/api`)
    /// for the §4 outbound `chat.postMessage` + thread re-read. Null in
    /// production. Integration tests point it at a loopback FakeSlack so the
    /// outbound post + thread fetch hit an in-process server, never real Slack.
    connector_slack_api_base_override: ?[]const u8 = null,
    /// Once-set cache for the platform Slack signing secret (admin-vault
    /// `slack-app`.`signing_secret`). The events ingress publishes its first
    /// successful vault read here and every later request borrows it — the
    /// steady-state per-event vault SELECT disappears, while an unconfigured
    /// deployment keeps re-reading until the operator vaults the secret (live
    /// vaulting needs no restart). Entry + bytes are `alloc`-owned; the Context
    /// owner frees via `deinitSlackSigningSecretCache`.
    connector_slack_signing_secret_cache: std.atomic.Value(?*const SlackSigningSecret) = .init(null),
    clerk_secret_key: ?[]const u8,
    oidc: ?*oidc.Verifier,
    /// Cloudflare R2 client for Fleet Bundle canonical-tar storage, resolved once
    /// at boot. Null when R2 credentials are unset (local dev / paste-only) — the
    /// import handler 503s only when a bundle actually has support files to store.
    r2: ?*R2 = null,
    auth_sessions: *session_store_redis.SessionStore,
    audit_ctx: audit_events.AuditCtx,
    app_url: []const u8,
    api_url: []const u8,
    api_in_flight_requests: std.atomic.Value(u32),
    api_max_in_flight_requests: u32,
    /// Optional integration-test probe updated after API admission. Production
    /// leaves it null; tests use it to prove handlers overlap server-side.
    api_peak_in_flight_probe: ?*std.atomic.Value(u32) = null,
    /// Ceiling for live SSE streams (SSE_MAX_STREAMS env knob, parsed in
    /// runtime_loader). Streams run on dedicated detached threads, so the cap
    /// bounds threads + memory — not handler-pool occupancy. Defaults so
    /// test/fixture Contexts that omit it get the production default.
    sse_max_streams: u32 = runtime_loader.SSE_MAX_STREAMS_DEFAULT,
    /// The process's shared Redis pub/sub fan-out — SSE streams subscribe
    /// through it instead of dialing per-stream connections. Boot-owned
    /// (serve.zig / TestHarness), started before the server listens.
    hub: *subscription_hub,
    /// Live SSE stream owner: cap admission, in-flight gauge, drain, listing. Boot-owned.
    stream_registry: *stream_registry,
    /// Per-workspace fleet sets, enumerated once per workspace per cadence, shared by every stream (V viewers → one enumeration).
    fleet_sets: *fleet_set_cache,
    ready_max_queue_depth: ?i64,
    ready_max_queue_age_ms: ?i64,
    telemetry: *telemetry_mod.Telemetry,
    /// Tenant balance-exhaustion gate policy, resolved once from the env at
    /// startup (the credit gate reads this, not the env, per request). Defaults
    /// so test/fixture Contexts that omit it get the production default.
    balance_policy: balance_policy.Policy = balance_policy.DEFAULT,
    /// On-demand credential broker (M102), a daemon singleton built at boot and
    /// shared across request threads. The credential-mint handler resolves a
    /// vault handle to a short-lived token through it. Optional + defaulted: test
    /// and fixture Contexts that omit it get null, and the mint handler fails
    /// closed (503) when the broker is unconfigured.
    broker: ?*CredentialBroker = null,
    /// Drain handle for detached install-progression workers (fleet create
    /// spawns one per fleet). serve.zig owns one in production and the test
    /// harness owns its own: both wait() on it BEFORE freeing the pool/queue
    /// the workers borrow — pool teardown IS graceful in production, so an
    /// unawaited worker would use-after-free on pool.acquire(). Null only in
    /// fixture Contexts that never reach the create path.
    install_wg: ?*constants.WaitGroup = null,

    pub const SlackSigningSecret = struct { bytes: []const u8 };

    /// The cached platform Slack signing secret, borrowed for the process
    /// lifetime — callers never free it. Null until the first successful
    /// publish.
    pub fn cachedSlackSigningSecret(self: *Context) ?[]const u8 {
        const entry = self.connector_slack_signing_secret_cache.load(.acquire) orelse return null;
        return entry.bytes;
    }

    /// Publish a freshly-loaded signing secret. Takes ownership of `owned`
    /// (allocated with `self.alloc`) and returns the canonical process-lifetime
    /// slice to use. Thread-safe: a compare-and-swap loser frees its copy and
    /// borrows the winner's. Null only when the entry allocation fails (`owned`
    /// has been freed) — the caller fails the request closed.
    pub fn publishSlackSigningSecret(self: *Context, owned: []const u8) ?[]const u8 {
        const entry = self.alloc.create(SlackSigningSecret) catch {
            self.alloc.free(owned);
            return null;
        };
        entry.* = .{ .bytes = owned };
        if (self.connector_slack_signing_secret_cache.cmpxchgStrong(null, entry, .release, .acquire)) |current| {
            // Lost the publish race — free ours, borrow the winner's canonical
            // bytes. `current` is non-null by construction (the CAS only fails
            // when the observed value differs from the expected null).
            self.alloc.free(owned);
            self.alloc.destroy(entry);
            return (current orelse return null).bytes;
        }
        return owned;
    }

    /// Free the cache entry (owner-side teardown: serve.zig defer + the test
    /// harness deinit). Safe on an empty cache and idempotent via swap.
    pub fn deinitSlackSigningSecretCache(self: *Context) void {
        if (self.connector_slack_signing_secret_cache.swap(null, .acq_rel)) |entry| {
            self.alloc.free(entry.bytes);
            self.alloc.destroy(@constCast(entry));
        }
    }
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

// AuthPrincipal lives in src/agentsfleetd/auth/; the handler layer reaches it
// through this re-export. The role ladder was removed —
// authorization is scope-based (see require_scope middleware + route_scopes).
pub const AuthPrincipal = principal_mod.AuthPrincipal;

pub fn writeJson(res: *httpz.Response, status: std.http.Status, value: anytype) void {
    res.status = @intFromEnum(status);
    res.json(value, .{}) catch {
        res.status = 500;
        res.body = S_PUNCT_99914B;
    };
}

// The RFC 7807 problem-response writers live in `problem_response.zig` (RULE
// FLL). Re-exported so every handler's `common.errorResponse(...)` /
// `common.internal*Error(...)` call site is unchanged.
pub const errorResponse = problem_response.errorResponse;
pub const errorResponseConflict = problem_response.errorResponseConflict;
pub const errorResponsePrecondition = problem_response.errorResponsePrecondition;
pub const internalDbUnavailable = problem_response.internalDbUnavailable;
pub const internalDbError = problem_response.internalDbError;
pub const internalOperationError = problem_response.internalOperationError;

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

/// Erase the mutable httpz-owned storage exposed through a const body view.
pub fn secureZeroRequestBody(body: []const u8) void {
    std.crypto.secureZero(u8, @constCast(body));
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

/// Write a 405 Method Not Allowed response.
/// Used by route_table.zig invoke functions that do their own method dispatch.
pub fn respondMethodNotAllowed(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.method_not_allowed);
    res.body = "";
}

/// Returns `true` when `method` matches `expected`. On a mismatch, writes a 405
/// via `respondMethodNotAllowed` and returns `false` — same side-effect-on-
/// failure + bool shape as `requireUuidV7Id`. Collapses the hand-copied
/// single-method check at a route_table invoke site to one line:
/// `if (!common.requireMethod(res, req.method, .POST)) return;`. Multi-method
/// routes keep their `switch (req.method)` dispatch.
pub fn requireMethod(res: *httpz.Response, method: httpz.Method, expected: httpz.Method) bool {
    if (method == expected) return true;
    respondMethodNotAllowed(res);
    return false;
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

pub const getFleetWorkspaceId = authz.getFleetWorkspaceId;
pub const authorizeWorkspace = authz.authorizeWorkspace;
pub const setTenantSessionContext = authz.setTenantSessionContext;
pub const authorizeWorkspaceAndSetTenantContext = authz.authorizeWorkspaceAndSetTenantContext;
pub const openHandlerTestConn = authz.openHandlerTestConn;

test {
    _ = @import("common_authz_test.zig");
}
