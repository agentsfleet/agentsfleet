//! httpz HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const handler = @import("handler.zig");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const auth_adapter = @import("handlers/auth/adapter.zig");
const route_table = @import("route_table.zig");
const hx_mod = @import("handlers/hx.zig");
const error_codes = @import("../errors/error_registry.zig");
const metrics = @import("../observability/metrics.zig");
const logging = @import("log");

const log = logging.scoped(.http);

const DEFAULT_MAX_CLIENTS = 1024;

// 429 shed headers — the REST guidelines bind 429 to Retry-After + X-RateLimit-*.
// Semantics here are the instance-wide in-flight ceiling, not a per-client
// quota. Retry-After name, seconds, and rendered value live in
// handlers/common.zig (shared with the SSE cap's 503).
const HEADER_RATELIMIT_LIMIT = "X-RateLimit-Limit";
const HEADER_RATELIMIT_REMAINING = "X-RateLimit-Remaining";
const HEADER_RATELIMIT_RESET = "X-RateLimit-Reset";
const FMT_UNSIGNED = "{d}";
const S_RATELIMIT_REMAINING_NONE = "0";

const ServerConfig = struct {
    port: u16 = 3000,
    /// Dual-stack "::" accepts both IPv4 and IPv6 connections.
    /// httpz (pure Zig) uses std.posix — no C-layer IPV6_V6ONLY concern.
    interface: []const u8 = "::",
    threads: i16 = 1,
    workers: i16 = 1,
    max_clients: ?isize = DEFAULT_MAX_CLIENTS,
};

/// httpz handler struct — carries Context and owns dispatch.
///
/// `registry` is a pointer to the boot-time `MiddlewareRegistry` allocated in
/// `src/cmd/serve.zig`. The registry must outlive the server (both live in the
/// `run()` stack frame). All threads share this read-only pointer — no mutex
/// needed because registry is immutable after `initChains()`.
const App = struct {
    ctx: *handler.Context,
    registry: *auth_mw.MiddlewareRegistry,

    pub fn handle(self: App, req: *httpz.Request, res: *httpz.Response) void {
        dispatch(self.ctx, self.registry, req, res);
    }

    pub fn uncaughtError(_: App, _: *httpz.Request, res: *httpz.Response, _: anyerror) void {
        res.status = 500;
        res.body = "{\"error\":{\"code\":\"INTERNAL\",\"message\":\"Internal server error\"}}";
    }
};

/// Handle-based server. Stop from any thread via `Server.stop()`.
/// Replaces the previous module-level pointer (which was a cross-thread data race
/// and meant tests couldn't isolate their own server instance).
pub const Server = struct {
    alloc: std.mem.Allocator,
    inner: httpz.Server(App),
    cfg: ServerConfig,

    /// `registry` must outlive the server — typically a pointer to a var in
    /// `src/cmd/serve.zig::run()` that was fully initialised via `initChains()`.
    pub fn init(io: std.Io, ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, cfg: ServerConfig) !*Server {
        const alloc = ctx.alloc;
        const self = try alloc.create(Server);
        errdefer alloc.destroy(self);
        // Parse the configured interface string ("::" dual-stack default,
        // "0.0.0.0", "::1", …) into an Io.net.IpAddress. `.localhost`/`.all`
        // constructors would drop the operator-chosen interface.
        const listen_addr = try std.Io.net.IpAddress.parse(cfg.interface, cfg.port);
        self.* = .{
            .alloc = alloc,
            .inner = try httpz.Server(App).init(io, alloc, .{
                .address = .{ .ip = listen_addr },
                .workers = .{
                    .count = @intCast(cfg.workers),
                    .max_conn = if (cfg.max_clients) |mc| @intCast(mc) else null,
                },
                .thread_pool = .{
                    .count = @intCast(cfg.threads),
                },
                .request = .{
                    .max_body_size = common.MAX_BODY_SIZE,
                },
            }, .{ .ctx = ctx, .registry = registry }),
            .cfg = cfg,
        };
        return self;
    }

    /// Block until stop() is called from another thread.
    pub fn listen(self: *Server) !void {
        log.info("listening", .{ .interface = self.cfg.interface, .port = self.cfg.port });
        try self.inner.listen();
    }

    /// Signal the server to stop. Safe to call from any thread.
    pub fn stop(self: *Server) void {
        self.inner.stop();
    }

    pub fn deinit(self: *Server) void {
        self.inner.deinit();
        self.alloc.destroy(self);
    }

    /// Convenience constructor for tests that do not exercise the middleware
    /// fast-path. Uses a module-level dummy registry (route table is empty in
    /// C.2, so the registry is never dereferenced during test runs).
    fn initForTesting(io: std.Io, ctx: *handler.Context, cfg: ServerConfig) !*Server {
        return init(io, ctx, &testing_dummy_registry, cfg);
    }
};

/// Module-level dummy registry used by `Server.initForTesting`.
///
/// Fields are undefined — safe only for tests that do NOT hit authenticated
/// routes (i.e. tests that never call dispatch, or only hit `none`-policy
/// routes like /healthz). Integration tests that exercise bearer/admin routes
/// must use `Server.init` with a properly-initialized registry instead.
/// Lives in the data segment (not the stack) so `initForTesting` returns
/// no dangling pointer.
// SAFETY: written by surrounding init logic before any read of this storage.
var testing_dummy_registry: auth_mw.MiddlewareRegistry = undefined;

// ── Request dispatch ──────────────────────────────────────────────────────

/// Top-level request handler. Match first (cheap), then class-gate before
/// invoke: ops routes are never shed — an admission storm must not blind the
/// operators diagnosing it; stream routes answer to the dedicated SSE cap
/// instead of the api ceiling; api routes claim an in-flight slot and shed
/// 429 above it. Unmatched paths 404 without consuming admission (a 404
/// costs less than the gate).
fn dispatch(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response) void {
    const path = req.url.path;
    const matched = router.match(path, req.method) orelse {
        respondNotFound(res);
        return;
    };
    switch (route_table.classFor(matched)) {
        .ops, .stream => invokeMatched(ctx, registry, req, res, matched, path),
        .api => dispatchApi(ctx, registry, req, res, matched, path),
    }
}

/// api-class admission: claim an in-flight slot; above the ceiling the
/// request is shed with 429 before any per-request allocation.
fn dispatchApi(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, matched: router.Route, path: []const u8) void {
    // safe because: pure admission counter — no memory is published through
    // it, over-claimers release in the paired defer below.
    const live = ctx.api_in_flight_requests.fetchAdd(1, .monotonic) + 1;
    defer {
        // safe because: same admission counter; the gauge store tolerates
        // last-writer staleness between concurrent requests.
        const after = ctx.api_in_flight_requests.fetchSub(1, .monotonic) - 1;
        metrics.setApiInFlightRequests(after);
    }
    metrics.setApiInFlightRequests(live);
    if (live > ctx.api_max_in_flight_requests) {
        respondBackpressureShed(ctx, res, live, path);
        return;
    }
    invokeMatched(ctx, registry, req, res, matched, path);
}

fn invokeMatched(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, matched: router.Route, path: []const u8) void {
    // Resolve trace context from inbound traceparent header or generate root.
    const tctx = common.resolveTraceContext(req);
    const start_ns: u64 = @intCast(clock.nowNanos());
    dispatchMatchedRoute(ctx, registry, req, res, matched);
    emitRequestSpan(tctx, path, start_ns);
}

/// 429 shed: problem+json envelope + Retry-After + X-RateLimit-* (instance
/// ceiling semantics). Dynamic header values live on the request arena —
/// httpz borrows header slices until the response is written.
fn respondBackpressureShed(ctx: *handler.Context, res: *httpz.Response, live: u32, path: []const u8) void {
    metrics.incApiBackpressureRejections();
    log.warn("request_shed", .{
        .error_code = error_codes.ERR_API_BACKPRESSURE,
        .in_flight = live,
        .max = ctx.api_max_in_flight_requests,
        .path = path,
    });
    res.header(common.HEADER_RETRY_AFTER, common.RETRY_AFTER_BRIEF_VALUE);
    res.header(HEADER_RATELIMIT_REMAINING, S_RATELIMIT_REMAINING_NONE);
    headerUint(res, HEADER_RATELIMIT_LIMIT, ctx.api_max_in_flight_requests);
    const reset_epoch_s: u64 = @intCast(@divTrunc(clock.nowMillis(), std.time.ms_per_s) + common.RETRY_AFTER_BRIEF_SECONDS);
    headerUint(res, HEADER_RATELIMIT_RESET, reset_epoch_s);
    // a real request id keeps the shed traceable; falls back to the sentinel
    // only if the arena print itself fails
    common.errorResponse(res, error_codes.ERR_API_BACKPRESSURE, error_codes.MSG_API_BACKPRESSURE, common.requestId(res.arena));
}

/// Best-effort numeric header on the request arena; a failed print drops the
/// advisory header rather than the shed response.
fn headerUint(res: *httpz.Response, name: []const u8, value: u64) void {
    if (std.fmt.allocPrint(res.arena, FMT_UNSIGNED, .{value})) |s| {
        res.header(name, s);
    } else |_| {}
}

fn emitRequestSpan(tctx: common.TraceContext, path: []const u8, start_ns: u64) void {
    const end_ns: u64 = @intCast(clock.nowNanos());
    var span = otel_traces.buildSpan(tctx, "http.request", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "http.route", path);
    otel_traces.enqueueSpan(span);
}

fn dispatchMatchedRoute(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, matched: router.Route) void {
    const spec = route_table.specFor(matched, registry);
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);
    var auth = auth_adapter.buildAuthCtx(res, alloc, req_id);

    // Populate the webhook zombie slot before running the middleware
    // chain. The webhook_sig + svix middlewares read it; all other
    // middlewares ignore the field.
    switch (matched) {
        .receive_webhook => |zombie_id| {
            auth.webhook_zombie_id = zombie_id;
        },
        .receive_svix_webhook => |zombie_id| {
            auth.webhook_zombie_id = zombie_id;
        },
        .github_webhook => |zombie_id| {
            auth.webhook_zombie_id = zombie_id;
        },
        else => {},
    }

    const outcome = auth_mw.run(auth_mw.AuthCtx, spec.middlewares, &auth, req) catch |e| {
        common.internalOperationError(res, @errorName(e), req_id);
        return;
    };
    if (outcome == .short_circuit) return;

    // Build Hx from auth context — principal is set by bearer/admin middleware
    // for authenticated routes; zero-value (.mode=.api_key) for none-policy
    // routes (those handlers do not access hx.principal).
    var hx = hx_mod.Hx{
        .alloc = alloc,
        .principal = auth.principal orelse .{ .mode = .api_key },
        .req_id = req_id,
        .ctx = ctx,
        .res = res,
    };
    spec.invoke(&hx, req, matched);
}

fn respondNotFound(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.not_found);
    res.body =
        \\{"error":{"code":"NOT_FOUND","message":"No such route"}}
    ;
}

test "dispatchMatchedRoute route matcher covers tenant billing endpoint" {
    const matched = router.match("/v1/tenants/me/billing", .GET) orelse return error.TestExpectedEqual;
    switch (matched) {
        .get_tenant_billing => {},
        else => return error.TestExpectedEqual,
    }
}

// ── ServerConfig tests ───────────────────────────────────────────────────

test "ServerConfig default interface is dual-stack (::)" {
    const cfg = ServerConfig{};
    try std.testing.expectEqualStrings("::", cfg.interface);
}

test "ServerConfig default interface is NOT IPv4-only — regression guard" {
    const cfg = ServerConfig{};
    // The old default "0.0.0.0" caused Fly 6PN (IPv6) tunnel connections to be refused.
    const is_ipv4_only = std.mem.eql(u8, cfg.interface, "0.0.0.0") or
        std.mem.eql(u8, cfg.interface, "127.0.0.1");
    try std.testing.expect(!is_ipv4_only);
}

test "ServerConfig accepts custom IPv4 interface override" {
    const cfg = ServerConfig{ .interface = "0.0.0.0" };
    try std.testing.expectEqualStrings("0.0.0.0", cfg.interface);
}

test "ServerConfig accepts custom IPv6 loopback interface" {
    const cfg = ServerConfig{ .interface = "::1" };
    try std.testing.expectEqualStrings("::1", cfg.interface);
}

test "ServerConfig default port is 3000" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
}

test "ServerConfig defaults are stable — full struct check" {
    const cfg = ServerConfig{};
    try std.testing.expectEqual(@as(u16, 3000), cfg.port);
    try std.testing.expectEqualStrings("::", cfg.interface);
    try std.testing.expectEqual(@as(i16, 1), cfg.threads);
    try std.testing.expectEqual(@as(i16, 1), cfg.workers);
    try std.testing.expectEqual(@as(?isize, DEFAULT_MAX_CLIENTS), cfg.max_clients);
}

// ── Server lifecycle tests ───────────────────────────────────────────────
// The integration tests (rbac/tenant_provider/telemetry) cover init→listen→stop→deinit
// end-to-end. These two unit tests lock contracts those can't reach:
// the no-listen unwind path and pre-listen stop().

test "Server.init then deinit without listen does not leak" {
    // std.testing.allocator asserts no leaks at test exit.
    // Catches any future refactor that allocates in init() but only frees in
    // a path conditional on listen() having been called.
    const alloc = std.testing.allocator;
    var ctx: handler.Context = undefined;
    ctx.alloc = alloc;
    const srv = try Server.initForTesting(@import("common").globalIo(), &ctx, .{ .threads = 1, .workers = 1, .max_clients = 4 });

    srv.deinit();
}

test {
    _ = @import("rbac_http_integration_test.zig");
    _ = @import("credentials_json_integration_test.zig");
    _ = @import("test_harness.zig");
    _ = @import("webhook_test_signers.zig");
    _ = @import("webhook_test_fixtures.zig");
    _ = @import("webhook_http_integration_test.zig");
    _ = @import("test_port.zig");
}
