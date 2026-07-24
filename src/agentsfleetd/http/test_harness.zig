// Test harness — supertest-style fluent API for in-process HTTP integration tests.
//
// Goal: replace the duplicated TestServer boilerplate in tenant_provider_http_integration_test.zig
// and rbac_http_integration_test.zig with one shared module. New integration tests
// (webhook, dashboard, telemetry, …) consume it directly.
//
// Contract:
//   * Runs against the LIVE test database. Never creates temp tables.
//   * Caller wires middleware via `configureRegistry` callback so every suite
//     can plug in only the auth policies it needs.
//   * Fixtures are inserted through `conn()` accessor and MUST be cleaned up
//     explicitly in the test body (not via defer) — mirrors the proven
//     rbac pattern where deferred cleanup leaks connections at pool.deinit.
//
// Shape:
//   var h = try TestHarness.start(alloc, .{ .configureRegistry = myWireFn });
//   defer h.deinit();
//   const r = try h.post("/v1/foo").header("x-sig", sig).json(body).send();
//   defer r.deinit();
//   try r.expectStatus(.accepted);
//
// This file is the harness core: `Config`, the `TestHarness` struct (lifecycle
// + verb dispatch). The fluent `Request`/`Response` types live in
// `test_http_message.zig` and the server bring-up plumbing in
// `test_harness_server.zig`; both are stitched back in here so consumers
// import everything from this one module.

const std = @import("std");
const constants = @import("common");
const call_deadline = @import("call_deadline");
const pg = @import("pg");
const session_store_redis = @import("../session/session_store_redis.zig");
const audit_events = @import("../auth/audit_events.zig");
const oidc = @import("../auth/oidc.zig");
const queue_redis = @import("../queue/redis.zig");
const auth_mw = @import("../auth/middleware/mod.zig");
const common = @import("handlers/common.zig");
const handler = @import("handler.zig");
const http_server = @import("server.zig");
const telemetry_mod = @import("../observability/telemetry.zig");
const subscription_hub = @import("../events/subscription_hub.zig");
const fleet_set_cache = @import("../events/fleet_set_cache.zig");
const stream_registry = @import("stream_registry.zig");
const message = @import("test_http_message.zig");
const server_bringup = @import("test_harness_server.zig");
const test_fixtures = @import("../db/test_fixtures.zig");

const TEST_AUTH_SESSION_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";
const TEST_AUDIT_LOG_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";

/// Reader-socket read timeout for the harness hub. Production uses the hub's
/// 1 s default; tests lower it because every `deinit()` joins the hub reader
/// thread, which is parked in `nextMessage` — at 1 s that join cost ~0.9 s PER
/// TEST and dominated the integration-suite wall-clock. Against localhost Redis
/// (instant, tiny pub/sub frames) a 100 ms SO_RCVTIMEO never truncates a frame,
/// and the dead-socket discrimination (read_timeout_ms / 2 = 50 ms) still holds.
const TEST_HUB_READ_TIMEOUT_MS: u32 = 100;
/// Workspace-stream refresh cadence under test: low enough that a fleet
/// appearing or a membership revoked surfaces within a test's patience, high
/// enough that a settle sleep can reliably straddle exactly one tick.
const TEST_FLEET_SET_REFRESH_MS: i64 = 150;

/// Re-exported from `test_http_message.zig` so consumers keep importing the
/// fluent request/response types from this module unchanged.
pub const Request = message.Request;
pub const Response = message.Response;

pub const Config = struct {
    /// Caller configures the middleware registry. Called AFTER init with pool
    /// already wired into ctx. Caller sets up policies it cares about; unused
    /// policies stay default (stubbed in the helper).
    configureRegistry: *const fn (reg: *auth_mw.MiddlewareRegistry, harness: *TestHarness) anyerror!void,
    /// Optional JWT verification — defaults to a no-op verifier for suites
    /// that don't exercise bearer auth.
    inline_jwks_json: []const u8 = DEFAULT_JWKS,
    issuer: []const u8 = "https://test.invalid",
    audience: []const u8 = "https://test.invalid",
    /// Max time to wait for the in-process server's `/healthz` to return 200
    /// before `start()` returns `error.ServerStartTimeout`. Default 4 s tolerates
    /// CI contention; raise for very slow runners.
    wait_timeout_ms: u32 = 4000,
};

const DEFAULT_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"test-kid-static","use":"sig","alg":"RS256"}]}
;

pub const TestHarness = struct {
    alloc: std.mem.Allocator,
    pool: *pg.Pool,
    session_store: session_store_redis.SessionStore,
    verifier: oidc.Verifier,
    queue: queue_redis.Client,
    has_redis: bool = false,
    /// Shared SSE fan-out, mirroring serve.zig's boot wiring. Init'd cold
    /// (no Redis) so every Context has a valid pointer; started inside
    /// `connectRedis` once the queue config exists. SSE suites gate on the
    /// Redis env anyway, so a cold hub only ever serves non-SSE routes.
    /// One deadline scheduler, mirroring serve.zig's single process worker.
    deadline_backend: call_deadline.MonotonicBackend,
    deadline_scheduler: call_deadline.ProcessScheduler,
    /// Concurrency-capable Io for the hub, mirroring the process Io that
    /// `serve.run` hands it. `globalIo()` is statically single-threaded, so a
    /// raced dial cannot run on it.
    hub_io: std.Io.Threaded,
    hub: subscription_hub,
    /// Live-stream owner, mirroring serve.zig. Cheap init, no Redis dependency.
    streams: stream_registry,
    /// Shared per-workspace fleet sets, mirroring serve.zig (lazy DB query).
    fleet_sets: fleet_set_cache,
    telemetry: telemetry_mod.Telemetry,
    registry: auth_mw.MiddlewareRegistry,
    ctx: handler.Context,
    server: *http_server.Server,
    thread: std.Thread,
    port: u16,
    /// Set by `serverThread` when httpz's bind loses the port to a sibling
    /// harness (the `allocFreePort` TOCTOU). Read by `waitForServer` so
    /// `bringUpServer` retries on a fresh port instead of the thread panicking
    /// the whole process.
    bind_failed: std.atomic.Value(bool) = .{ .raw = false },
    /// Drains detached install-progression workers (spawned by fleet create)
    /// before `deinit()` frees the pool + queue they hold. Without it a worker
    /// mid-`flipToActive` calls `pool.acquire()` on a freed pool → segfault — a
    /// timing-flaky crash under the parallel test runner. `ctx.install_wg` points
    /// here so `create_install_steps.spawn` registers each worker.
    install_wg: constants.WaitGroup = .{},

    /// Connect Redis using the same env var `serve.zig` uses (REDIS_URL_API,
    /// role .api) — the harness simulates the API server. Returns the raw
    /// error so the caller can distinguish env-missing (skip) from
    /// connect-failed (hard fail in CI). `make test-integration` exports
    /// REDIS_URL_API automatically; local dev without Redis sees
    /// `error.MissingRedisUrl` and the harness escalates to SkipZigTest.
    pub fn connectRedis(self: *TestHarness) !void {
        if (self.has_redis) return;
        var env_map = try constants.env.testLiveSnapshot(self.alloc);
        defer env_map.deinit();
        self.queue = try queue_redis.Client.connectFromEnv(constants.globalIo(), &env_map, self.alloc, .api);
        self.has_redis = true;
        // Lower the reader-socket timeout BEFORE start() so the reader thread is
        // created with it — bounds the per-test `deinit()` join (see the const).
        self.hub.read_timeout_ms = TEST_HUB_READ_TIMEOUT_MS;
        try self.hub.start(self.queue.pool.cfg, &self.deadline_scheduler);
        // SessionStore holds a pointer to `self.queue`; safe now that the
        // queue handle is initialized. `ctx.auth_sessions` already points
        // to `&self.session_store`, so the in-place init is observed by
        // every handler call from this point on.
        self.session_store = session_store_redis.SessionStore.init(
            self.alloc,
            &self.queue,
            TEST_AUTH_SESSION_PEPPER,
            TEST_AUDIT_LOG_PEPPER,
        );
    }

    /// Bool-returning wrapper kept for tests that opt into Redis
    /// opportunistically (`if (!h.tryConnectRedis()) return error.SkipZigTest;`
    /// and `_ = h.tryConnectRedis();`). New code should call `connectRedis`
    /// directly to inspect the error variant.
    pub fn tryConnectRedis(self: *TestHarness) bool {
        self.connectRedis() catch return false;
        return true;
    }

    /// Start an in-process server on a free port. Returns `error.SkipZigTest`
    /// when the test DB is not configured (`TEST_DATABASE_URL` unset).
    pub fn start(alloc: std.mem.Allocator, cfg: Config) !*TestHarness {
        const db_ctx = (try common.openHandlerTestConn(alloc)) orelse return error.SkipZigTest;
        // Clear any fault-injection constraint a killed reclaim run leaked, before
        // this (or any) fleet test touches the shared tables — see the fn's note.
        test_fixtures.dropInjectedFaultConstraints(db_ctx.conn);
        db_ctx.pool.release(db_ctx.conn);
        // Ownership transfers to h.pool below, but until we successfully
        // return h the pool is THIS function's responsibility — without
        // this errdefer, any later failure (server init, spawn,
        // waitForServer, tryConnectRedis) leaks the pool's Postgres
        // connections. Cascading test failures hit `sorry, too many
        // clients already` after ~25 leaked harnesses (5-8 conns each).
        errdefer db_ctx.pool.deinit();

        const h = try alloc.create(TestHarness);
        errdefer alloc.destroy(h);

        h.* = .{
            .alloc = alloc,
            .pool = db_ctx.pool,
            // SAFETY: SessionStore is populated in-place by `connectRedis()` below
            // (needs the queue handle). `ctx.auth_sessions = &h.session_store`
            // captures the stable pointer; the pointee is initialized before any
            // auth-session handler reads it.
            .session_store = undefined,
            .verifier = oidc.Verifier.init(alloc, .{
                .provider = .clerk,
                .jwks_url = "https://test.invalid/jwks",
                .issuer = cfg.issuer,
                .audience = cfg.audience,
                .inline_jwks_json = cfg.inline_jwks_json,
            }),
            // SAFETY: test fixture; field is populated by the surrounding builder before any read.
            .queue = undefined,
            .hub_io = std.Io.Threaded.init(alloc, .{}),
            // SAFETY: repointed at the harness-owned Io immediately below, once
            // `h` has its final address.
            .hub = undefined,
            .streams = stream_registry.init(alloc, constants.globalIo()),
            .fleet_sets = blk: {
                var fs = fleet_set_cache.init(alloc, constants.globalIo());
                fs.refresh_interval_ms = TEST_FLEET_SET_REFRESH_MS;
                break :blk fs;
            },
            .telemetry = telemetry_mod.Telemetry.initTest(),
            .deadline_backend = .{},
            // SAFETY: replaced with a started scheduler immediately below,
            // once `h` has its final address (the backend pointer must be stable).
            .deadline_scheduler = undefined,
            // SAFETY: test fixture; field is populated by the surrounding builder before any read.
            .registry = undefined,
            // SAFETY: test fixture; field is populated by the surrounding builder before any read.
            .ctx = undefined,
            // SAFETY: test fixture; field is populated by the surrounding builder before any read.
            .server = undefined,
            // SAFETY: test fixture; field is populated by the surrounding builder before any read.
            .thread = undefined,
            // SAFETY: assigned from bringUpServer's bound port before the server is used.
            .port = undefined,
        };
        // Mirror deinit()'s teardown order from here forward — each
        // resource that gets a deinit there gets an errdefer here.
        h.hub = subscription_hub.init(alloc, h.hub_io.io());
        h.deadline_scheduler = call_deadline.ProcessScheduler.init(alloc, &h.deadline_backend);
        try h.deadline_scheduler.start();
        errdefer h.deadline_scheduler.deinit();

        errdefer h.verifier.deinit();

        h.ctx = .{
            .pool = h.pool,
            .queue = &h.queue,
            .alloc = alloc,
            // Threaded test io seam (sockets, jwks/SSE dials) — mirrors how
            // `serve.run` threads the real io onto the production Context.
            .io = constants.globalIo(),
            .deadline_scheduler = &h.deadline_scheduler,
            // Boot-resolved secrets default unset; a test that exercises a
            // secret-gated path sets the field on `&h.ctx` before its request
            // (Option-C convention — no `setenv`, which the 0.16 env snapshot
            // ignores). Null here keeps every other handler failing closed.
            .clerk_webhook_secret = null,
            .approval_signing_secret = null,
            .clerk_secret_key = null,
            .oidc = &h.verifier,
            .auth_sessions = &h.session_store,
            .audit_ctx = audit_events.AuditCtx.init(TEST_AUDIT_LOG_PEPPER),
            .app_url = "http://127.0.0.1",
            .api_url = "http://127.0.0.1",
            .api_in_flight_requests = std.atomic.Value(u32).init(0),
            .api_max_in_flight_requests = 64,
            .hub = &h.hub,
            .stream_registry = &h.streams,
            .fleet_sets = &h.fleet_sets,
            .ready_max_queue_depth = null,
            .ready_max_queue_age_ms = null,
            .telemetry = &h.telemetry,
            .install_wg = &h.install_wg,
        };
        h.registry = server_bringup.defaultRegistry(h, cfg);
        try cfg.configureRegistry(&h.registry, h);
        h.registry.initChains();
        h.port = try server_bringup.bringUpServer(h, alloc, cfg);
        // bringUpServer owns teardown of its own failed attempts; from here the
        // live server is the caller's. LIFO: stop+join runs before deinit
        // (mirrors deinit()'s order). Re-stopping a stopped server double-writes
        // httpz's close_fd → segfault, so these never overlap a manual stop.
        errdefer h.server.deinit();
        errdefer {
            h.server.stop();
            h.thread.join();
        }
        // Wire the queue upfront so handlers that publish (PATCH agent
        // status, webhooks, approvals, etc.) don't dereference undefined
        // memory. Three-way split:
        //   • `error.MissingRedisUrl` (REDIS_URL_API unset)  → SkipZigTest.
        //   • Any other error AND `CI` env unset (local dev) → SkipZigTest.
        //     The Makefile defaults `REDIS_URL_API` to a localhost rediss://
        //     URL even when no Redis is running; without this branch local
        //     `make test-integration` cascades into hundreds of failures.
        //   • Any other error AND `CI` env set (real CI run) →
        //     `error.RedisRequiredForTestHarness`. Silent skip in CI would
        //     mask coverage gaps — fail-hard is the intentional signal.
        // The errdefers above own teardown. A manual cleanup chain here
        // would double-call server.stop() (once explicit, once via
        // errdefer); the second write to a closed close_fd is a segfault
        // under httpz's eventfd-based shutdown.
        h.connectRedis() catch |err| switch (err) {
            error.MissingRedisUrl => return error.SkipZigTest,
            else => {
                if (constants.env.testLiveValue("CI")) |v| {
                    if (v.len > 0) return error.RedisRequiredForTestHarness;
                }
                return error.SkipZigTest;
            },
        };
        return h;
    }

    pub fn deinit(self: *TestHarness) void {
        self.server.stop();
        self.thread.join();
        self.server.deinit();
        // Stream teardown choreography (mirrors serve.zig): drain shuts the
        // client sockets (wakes write-blocked threads), hub.stop's close
        // broadcast wakes pop-parked threads, awaitEmpty bounds the wait for
        // their deregistration — then storage teardown is race-free.
        self.streams.drain();
        // Hub before queue: its connection + borrowed pool config must not
        // outlive the queue client. stop() is a no-op on a never-started hub.
        self.hub.stop();
        self.streams.awaitEmpty();
        self.hub.deinit();
        self.streams.deinit();
        // awaitEmpty above guarantees no stream thread survives to hold a
        // fleet-set reference (mirrors serve_shutdown.deinitStreaming).
        self.fleet_sets.deinit();
        self.verifier.deinit();
        // Server threads are joined above, so no request can be mid-publish;
        // frees the lazily-cached Slack signing secret (ctx.alloc-owned).
        self.ctx.deinitSlackSigningSecretCache();
        // The server is fully stopped + deinit'd above, so no handler can spawn
        // another install worker. Drain any already-detached worker before
        // freeing the queue + pool it borrows — a worker mid-flipToActive
        // otherwise acquires a freed pool (segfault, flaky under the parallel
        // runner). page/c-allocator-backed job memory is process-stable, so only
        // the queue + pool lifetimes need this barrier.
        self.install_wg.wait();
        // Every network user is joined above, so no interrupt callback can
        // still be running against a registration this frees.
        self.deadline_scheduler.deinit();
        self.hub_io.deinit();
        // Redis-backed SessionStore is a pure facade — no per-instance
        // teardown. `queue.deinit()` below releases the underlying pool.
        if (self.has_redis) self.queue.deinit();
        self.pool.deinit();
        self.alloc.destroy(self);
    }

    /// Acquire a connection for fixture insertion / cleanup. Caller releases
    /// back via `pool.release(conn)`.
    pub fn acquireConn(self: *TestHarness) !*pg.Conn {
        return self.pool.acquire();
    }
    pub fn releaseConn(self: *TestHarness, conn: *pg.Conn) void {
        self.pool.release(conn);
    }

    // ── Fluent request API ────────────────────────────────────────────────
    pub fn get(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .GET, path);
    }
    pub fn post(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .POST, path);
    }
    pub fn put(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .PUT, path);
    }
    pub fn patch(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .PATCH, path);
    }
    pub fn delete(self: *TestHarness, path: []const u8) Request {
        return Request.init(self, .DELETE, path);
    }
    pub fn request(self: *TestHarness, method: std.http.Method, path: []const u8) Request {
        return Request.init(self, method, path);
    }
};
