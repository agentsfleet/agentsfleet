//! httpz request routing; handlers run here and never block on fleet execution.

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
const route_scopes = @import("route_scopes.zig");
const hx_mod = @import("handlers/hx.zig");
const error_codes = @import("../errors/error_registry.zig");
const metrics = @import("../observability/metrics.zig");
const metrics_trace = @import("../observability/metrics_trace.zig");
const route_trace = @import("route_trace.zig");
const ZeroizingAllocator = @import("../secrets/zeroizing_allocator.zig");
const sensitive_request = @import("sensitive_request.zig");
const logging = @import("log");

const log = logging.scoped(.http);

const DEFAULT_MAX_CLIENTS = 1024;

/// Room for a request's status line and headers. httpz defaults to 4 KiB and
/// answers 431 past it — the narrowest header limit anywhere in the
/// production chain, and smaller than a real authenticated request can be:
/// a session bearer token runs past a kilobyte on its own, and each proxy the
/// request crosses appends its own forwarding and tracing headers before this
/// server reads them. Because the dashboard proxy returns the upstream status
/// verbatim, a refusal born here surfaces in a browser as a 431 against a
/// request whose own headers were small.
///
/// 16 KiB matches the default the Node proxy in front of this server already
/// tolerates, so this server stops being the tightest limit in the chain.
/// The cost is bounded: buffers are per connection, and only `min_conn` of
/// them are allocated ahead of demand.
const MAX_REQUEST_HEADER_BYTES: usize = 16 * 1024;
pub const S_AUTH_MW_FAILURE_DETAIL = "Could not verify the request. Try again; if it persists, contact support.";

// Instance-wide shed headers; shared retry values live in handlers/common.zig.
const HEADER_RATELIMIT_LIMIT = "X-RateLimit-Limit";
const HEADER_RATELIMIT_REMAINING = "X-RateLimit-Remaining";
const HEADER_RATELIMIT_RESET = "X-RateLimit-Reset";
const FMT_UNSIGNED = "{d}";
const S_RATELIMIT_REMAINING_NONE = "0";
const REQUEST_SPAN_NAME = "http.request";
const ATTR_HTTP_ROUTE = "http.route";

const RequestTrace = struct {
    context: common.TraceContext,
    wall_start_ns: u64,
    boot_start_ns: i96,
};

pub const ServerConfig = struct {
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
    const Self = @This();

    ctx: *handler.Context,
    registry: *auth_mw.MiddlewareRegistry,

    pub fn handle(self: Self, req: *httpz.Request, res: *httpz.Response) void {
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
    const Self = @This();

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
                    .buffer_size = MAX_REQUEST_HEADER_BYTES,
                },
            }, .{ .ctx = ctx, .registry = registry }),
            .cfg = cfg,
        };
        return self;
    }

    /// Block until stop() is called from another thread.
    pub fn listen(self: *Self) !void {
        log.debug("listening", .{ .interface = self.cfg.interface, .port = self.cfg.port });
        try self.inner.listen();
    }

    /// Signal the server to stop. Safe to call from any thread.
    pub fn stop(self: *Self) void {
        self.inner.stop();
    }

    pub fn deinit(self: *Self) void {
        self.inner.deinit();
        self.alloc.destroy(self);
    }
};

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
    const request_trace = beginRequestTrace(ctx.io, req);
    defer finishRequestTrace(ctx.io, matched, res.status, path, request_trace);
    switch (route_table.classFor(matched)) {
        .ops, .stream => dispatchMatchedRoute(ctx, registry, req, res, matched),
        .api => dispatchApi(ctx, registry, req, res, matched, path),
    }
}

/// api-class admission: claim an in-flight slot; above the ceiling the
/// request is shed with 429 before any per-request allocation.
fn dispatchApi(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, matched: router.Route, path: []const u8) void {
    // safe because: pure admission counter; over-claimers release below.
    const live = ctx.api_in_flight_requests.fetchAdd(1, .monotonic) + 1;
    if (ctx.api_peak_in_flight_probe) |probe| _ = probe.fetchMax(live, .monotonic);
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
    dispatchMatchedRoute(ctx, registry, req, res, matched);
}

fn beginRequestTrace(io: std.Io, req: *httpz.Request) RequestTrace {
    return .{
        .context = common.resolveTraceContext(req),
        .wall_start_ns = @intCast(clock.nowNanos()),
        .boot_start_ns = std.Io.Clock.boot.now(io).toNanoseconds(),
    };
}

fn finishRequestTrace(io: std.Io, route: router.Route, status: u16, path: []const u8, lifetime: RequestTrace) void {
    const boot_end_ns = std.Io.Clock.boot.now(io).toNanoseconds();
    const monotonic_second: u64 = @intCast(@divTrunc(lifetime.boot_start_ns, std.time.ns_per_s));
    switch (route_trace.decide(route, status, &lifetime.context.span_id, monotonic_second)) {
        .emit => emitRequestSpan(
            lifetime.context,
            path,
            lifetime.wall_start_ns,
            route_trace.endEpochNanos(lifetime.wall_start_ns, lifetime.boot_start_ns, boot_end_ns),
        ),
        .suppress => |reason| metrics_trace.inc(reason),
    }
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

fn emitRequestSpan(tctx: common.TraceContext, path: []const u8, start_ns: u64, end_ns: u64) void {
    var span = otel_traces.buildSpan(tctx, REQUEST_SPAN_NAME, start_ns, end_ns);
    _ = otel_traces.addAttr(&span, ATTR_HTTP_ROUTE, path);
    otel_traces.enqueueSpan(span);
}

fn dispatchMatchedRoute(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, matched: router.Route) void {
    const spec = route_table.specFor(matched, registry);
    defer sensitive_request.eraseAfterDispatch(req, matched);
    var zeroing = ZeroizingAllocator.wrap(ctx.alloc);
    var arena = std.heap.ArenaAllocator.init(zeroing.allocator());
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);
    var auth = auth_adapter.buildAuthCtx(res, alloc, req_id);

    // Per-request route capability requirement read by requireScope (empty for
    // no-auth/webhook routes). Resolved here so the auth layer never imports the
    // HTTP route table (portability boundary).
    auth.required_scopes = route_scopes.requiredScopes(matched, req.method);

    // Webhook fleet slot — read by webhook_sig + svix, ignored by others.
    switch (matched) {
        .receive_webhook, .receive_svix_webhook, .github_webhook => |fleet_id| {
            auth.webhook_fleet_id = fleet_id;
        },
        else => {},
    }

    const outcome = auth_mw.run(auth_mw.AuthCtx, spec.middlewares, &auth, req) catch |e| {
        log.err("auth_mw_chain_failed", .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(e) });
        common.internalOperationError(res, S_AUTH_MW_FAILURE_DETAIL, req_id); // mudball-ok: tag logged above, never returned
        return;
    };
    if (outcome == .short_circuit) return;

    // Principal is set by the bearer/runner middleware; zero-value for
    // none-policy routes (those handlers do not access hx.principal).
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

test {
    _ = @import("rbac_http_integration_test.zig");
    _ = @import("secrets_json_integration_test.zig");
    _ = @import("test_harness.zig");
    _ = @import("webhook_test_signers.zig");
    _ = @import("webhook_test_fixtures.zig");
    _ = @import("webhook_http_integration_test.zig");
    _ = @import("auth_mw_failure_integration_test.zig");
    _ = @import("request_header_size_integration_test.zig");
    _ = @import("test_port.zig");
    // M102 §3 — credential-mint handler unit tests (outcome→wire mapping).
    _ = @import("handlers/runner/credentials_mint.zig");
    // Declarative route → required-scope table tests.
    _ = @import("route_scopes_test.zig");
    // The server module's own unit tests (config defaults, lifecycle unwind).
    _ = @import("server_test.zig");
}
