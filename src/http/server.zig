//! httpz HTTP server setup and request routing.
//! Thread 1 — all endpoint handlers run here. Never blocks on agent execution.

const std = @import("std");
const httpz = @import("httpz");
const handler = @import("handler.zig");
const router = @import("router.zig");
const common = @import("handlers/common.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace_mod = @import("../observability/trace.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const auth_adapter = @import("handlers/auth_adapter.zig");
const route_table = @import("route_table.zig");
const hx_mod = @import("handlers/hx.zig");
const log = std.log.scoped(.http);

pub const ServerConfig = struct {
    port: u16 = 3000,
    /// Dual-stack "::" accepts both IPv4 and IPv6 connections.
    /// httpz (pure Zig) uses std.posix — no C-layer IPV6_V6ONLY concern.
    interface: []const u8 = "::",
    threads: i16 = 1,
    workers: i16 = 1,
    max_clients: ?isize = 1024,
};

/// httpz handler struct — carries Context and owns dispatch.
///
/// M18_002 C.2: `registry` is a pointer to the boot-time `MiddlewareRegistry`
/// allocated in `src/cmd/serve.zig`. The registry must outlive the server
/// (both live in the `run()` stack frame). All threads share this read-only
/// pointer — no mutex needed because registry is immutable after `initChains()`.
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
    pub fn init(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, cfg: ServerConfig) !*Server {
        const alloc = ctx.alloc;
        const self = try alloc.create(Server);
        errdefer alloc.destroy(self);
        self.* = .{
            .alloc = alloc,
            .inner = try httpz.Server(App).init(alloc, .{
                .address = .{ .ip = .{ .host = cfg.interface, .port = cfg.port } },
                .workers = .{
                    .count = @intCast(cfg.workers),
                    .max_conn = if (cfg.max_clients) |mc| @intCast(mc) else null,
                },
                .thread_pool = .{
                    .count = @intCast(cfg.threads),
                },
                .request = .{
                    .max_body_size = 2 * 1024 * 1024, // 2MB
                },
            }, .{ .ctx = ctx, .registry = registry }),
            .cfg = cfg,
        };
        return self;
    }

    /// Block until stop() is called from another thread.
    pub fn listen(self: *Server) !void {
        log.info("http.listening interface={s} port={d}", .{ self.cfg.interface, self.cfg.port });
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
    pub fn initForTesting(ctx: *handler.Context, cfg: ServerConfig) !*Server {
        return init(ctx, &testing_dummy_registry, cfg);
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
var testing_dummy_registry: auth_mw.MiddlewareRegistry = undefined;

// ── Request dispatch ──────────────────────────────────────────────────────

/// Top-level request handler — dispatches based on method + path prefix.
fn dispatch(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response) void {
    const path = req.url.path;

    // Resolve trace context from inbound traceparent header or generate root.
    const tctx = common.resolveTraceContext(req);
    const start_ns: u64 = @intCast(std.time.nanoTimestamp());

    if (dispatchMatchedRoute(ctx, registry, req, res, path)) {
        emitRequestSpan(tctx, path, start_ns);
        return;
    }
    respondNotFound(res);
}

fn emitRequestSpan(tctx: common.TraceContext, path: []const u8, start_ns: u64) void {
    const end_ns: u64 = @intCast(std.time.nanoTimestamp());
    var span = otel_traces.buildSpan(tctx, "http.request", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "http.route", path);
    otel_traces.enqueueSpan(span);
}

fn dispatchMatchedRoute(ctx: *handler.Context, registry: *auth_mw.MiddlewareRegistry, req: *httpz.Request, res: *httpz.Response, path: []const u8) bool {
    if (handler.parseSkillSecretRoute(path)) |route| {
        switch (req.method) {
            .PUT => handler.handlePutWorkspaceSkillSecret(ctx, req, res, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            .DELETE => handler.handleDeleteWorkspaceSkillSecret(ctx, req, res, route.workspace_id, route.skill_ref_encoded, route.key_name_encoded),
            else => respondMethodNotAllowed(res),
        }
        return true;
    }

    const matched = router.match(path) orelse return false;

    // M18_002 Batch D: route_table.specFor covers all Route variants.
    // The legacy switch is removed — all routes are handled here.
    if (route_table.specFor(matched, registry)) |spec| {
        var arena = std.heap.ArenaAllocator.init(ctx.alloc);
        defer arena.deinit();
        const alloc = arena.allocator();
        const req_id = common.requestId(alloc);
        var auth = auth_adapter.buildAuthCtx(res, alloc, req_id);

        // M28_001: populate webhook auth slots from route params before
        // running the middleware chain. The webhook_sig middleware reads these.
        switch (matched) {
            .receive_webhook => |wh| {
                auth.webhook_zombie_id = wh.zombie_id;
                auth.webhook_provided_secret = wh.secret;
            },
            else => {},
        }

        const outcome = auth_mw.run(auth_mw.AuthCtx, spec.middlewares, &auth, req) catch |e| {
            common.internalOperationError(res, @errorName(e), req_id);
            return true;
        };
        if (outcome == .short_circuit) return true;

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
        return true;
    }
    return false;
}

fn respondMethodNotAllowed(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.method_not_allowed);
    res.body = "";
}

fn respondNotFound(res: *httpz.Response) void {
    res.status = @intFromEnum(std.http.Status.not_found);
    res.body =
        \\{"error":{"code":"NOT_FOUND","message":"No such route"}}
    ;
}

test "dispatchMatchedRoute route matcher covers billing event endpoint" {
    const matched = router.match("/v1/workspaces/ws_1/billing/events") orelse return error.TestExpectedEqual;
    switch (matched) {
        .apply_workspace_billing_event => |workspace_id| try std.testing.expectEqualStrings("ws_1", workspace_id),
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
    try std.testing.expectEqual(@as(?isize, 1024), cfg.max_clients);
}

// ── Server lifecycle tests ───────────────────────────────────────────────
// The 3 integration tests (rbac/byok/telemetry) cover init→listen→stop→deinit
// end-to-end. These two unit tests lock contracts those can't reach:
// the no-listen unwind path and pre-listen stop().

test "Server.init then deinit without listen does not leak" {
    // T11 — std.testing.allocator asserts no leaks at test exit.
    // Catches any future refactor that allocates in init() but only frees in
    // a path conditional on listen() having been called.
    const alloc = std.testing.allocator;
    var ctx: handler.Context = undefined;
    ctx.alloc = alloc;
    const srv = try Server.initForTesting(&ctx, .{ .threads = 1, .workers = 1, .max_clients = 4 });
    srv.deinit();
}

test {
    _ = @import("rbac_http_integration_test.zig");
    _ = @import("byok_http_integration_test.zig");
    _ = @import("test_port.zig");
}
