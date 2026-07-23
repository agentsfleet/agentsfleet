//! Full-daemon boot -> SIGTERM -> drain lifecycle proof.
//!
//! Drives the REAL `serve.run` on a thread against live Postgres + TLS Redis,
//! holds one authenticated Server-Sent Events (SSE) stream open, delivers
//! SIGTERM through the real installed handler, and asserts the production
//! teardown choreography runs to completion: the SSE client hits End-Of-File
//! (EOF) (`streams.drain()` shut its live fd), `serve.run` returns and joins
//! within a bound, the listener refuses new connections, and
//! `std.testing.allocator` reports zero leaks over the whole boot + teardown.
//!
//! Why the real `serve.run` and not `TestHarness`: `TestHarness.deinit` is a
//! hand-maintained MIRROR of `serve.zig`'s teardown order, so asserting it can
//! never catch a regression in `serve.zig`'s real defer sequence — the exact
//! defect class the shutdown-race fixes closed. This is the only test that
//! exercises the production path end to end.
//!
//! Boot witness is `/healthz` (unauthenticated), NOT a server-published event:
//! httpz binds INSIDE `listen()`, which runs AFTER the server is published, so
//! a publish witness can fire for a server that immediately lost the port race
//! and died. A 200 also proves the signal handler is installed (`serve.run`
//! installs it before server init). SIGTERM is raised ONLY after that 200 — a
//! raise before the handler is installed would hit the default disposition and
//! kill the whole test binary.
//!
//! Env-gated + boot-probed: `serve.run` calls `std.process.exit(1)` on any boot
//! failure, which would kill the whole test binary, so the test SKIPS unless a
//! live, migrated Postgres and a reachable TLS Redis are proven FIRST.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const pg = @import("pg");
const serve = @import("serve.zig");
const serve_shutdown = @import("serve_shutdown.zig");
const cmd_common = @import("common.zig");
const db = @import("../db/pool.zig");
const queue_redis = @import("../queue/redis.zig");
const api_key = @import("../auth/api_key.zig");
const tenant_api_key = @import("../auth/middleware/tenant_api_key.zig");
const base = @import("../db/test_fixtures.zig");
const test_port = @import("../http/test_port.zig");
const SseClient = @import("../http/handlers/fleets/test_sse_client.zig");

// ── Bounded waits (all named per RULE UFS; wall-clock, not valgrind-scaled) ──
const BOOT_TIMEOUT_MS: u32 = 120_000; // ceiling for the server to serve /healthz
const HEALTHZ_POLL_MS: u32 = 25; // gap between boot-witness probes
const HEALTHZ_READ_MS: u32 = 1_000; // SO_RCVTIMEO on a single /healthz probe
const SHUTDOWN_NS: u64 = 60 * std.time.ns_per_s; // ceiling for run() to return after SIGTERM
const SSE_DEADLINE_MS: u32 = 30_000; // SO_RCVTIMEO on the held SSE stream's reads
const MAX_BOOT_ATTEMPTS: usize = 3; // fresh port per attempt (allocFreePort TOCTOU)
const EOF_MAX_FRAMES: usize = 128; // drain any buffered frames before EOF; bound the loop

// ── Fixed strings ────────────────────────────────────────────────────────────
const SERVE_ARGV0 = "agentsfleetd";
const SERVE_SUBCMD = "serve";
const PORT_ARG_BUF: usize = 24;
const HEALTHZ_REQ_BUF: usize = 128;
const HEALTHZ_RESP_BUF: usize = 64;
const HEALTHZ_REQUEST = "GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
const HTTP_200 = "200";
const SSE_PATH_FMT = "/v1/workspaces/{s}/fleets/{s}/events/stream";
const IGNORED_ERR_FMT = "lifecycle cleanup ignored: {s}";
const MS_PER_SECOND: u32 = 1000; // millisecond -> second divisor for the SO_RCVTIMEO timeval
const US_PER_MS: u32 = 1000; // millisecond -> microsecond factor for the timeval usec field

// Live-infra OS env keys the make integration lane exports (the ONLY values
// read from the live environment — the rest are injected literals so a dev
// machine's exporter/analytics env can never leak into the booted daemon).
// Isolation gate: this test drives a REAL full daemon (HTTP listener + sweepers
// + event bus + redis + telemetry captures + SIGTERM) — far too invasive to
// interleave with the ~2000 tests in the shared `make test-integration` binary,
// where it perturbs process-global state (the telemetry ring records a
// `.server_started`, the model rate cache is deinit'd, redis connections churn)
// and destabilizes unrelated tests. It runs ONLY in its own isolated, filtered
// process — the `make memleak` boot-drain lane sets this env; the general
// integration suite does not, so there it skips.
const LIFECYCLE_ISOLATION_ENV: [:0]const u8 = "AGENTSFLEET_LIFECYCLE_ISOLATED";
const OS_DB_URL_ENV: [:0]const u8 = "TEST_DATABASE_URL";
const OS_REDIS_TLS_URL_ENV: [:0]const u8 = "TEST_REDIS_TLS_URL";
const OS_REDIS_CA_CERT_ENV: [:0]const u8 = "REDIS_TLS_CA_CERT_FILE";
const REDIS_URL_API_ENV = "REDIS_URL_API";
const REDIS_CA_CERT_ENV = "REDIS_TLS_CA_CERT_FILE";

// Injected daemon config. Master key = the canonical TEST_KEK_HEX so the
// process-global Key-Encryption Key (KEK) this boot resolves matches what
// `crypto_primitives.setTestKek()` seeds for sibling tests. Peppers are 64-hex
// (the loader requires exactly 64 hex chars).
const ENCRYPTION_MASTER_KEY = "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";
const SESSION_PEPPER = "b" ** 64;
const AUDIT_PEPPER = "c" ** 64;
const OIDC_ISSUER = "https://oidc.lifecycle.test.invalid/";
const OIDC_AUDIENCE = "agentsfleetd-lifecycle-test";
const OIDC_PROVIDER = "clerk";

// Suite-private seed identifiers (uuidv7; version nibble 7 satisfies the CHECK).
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a5101";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a5111";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a5121";
const API_KEY_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a5131";
const TENANT_NAME = "LifecycleTestTenant";
const FLEET_NAME = "lifecycle-fleet";
const FLEET_CONFIG_JSON = "{}";
const FLEET_SOURCE_MD = "";
const TENANT_KEY_BODY_CHARS: usize = 48;
// Tenant API key (agt_t…) — resolved by the real serve.run's DB-backed
// api_key_lookup, so no JSON Web Key Set (JWKS) fetch is needed to authenticate
// the SSE stream (the workspace fleet-events route needs only FLEET_READ, which
// the tenant key bundle carries).
const AGT_T_KEY = tenant_api_key.TENANT_KEY_PREFIX ++ "d" ** TENANT_KEY_BODY_CHARS;

const BootOutcome = enum { served, died_early, timed_out };

const RunArgs = struct {
    io: std.Io,
    env_map: *const common.env.Map,
    argv: []const [:0]const u8,
    alloc: std.mem.Allocator,
    run_done: *common.Event,
    run_err: *?anyerror,
};

/// Thread body: run the real daemon, record any returned error, then signal
/// `run_done` so the test can bound the join (a hung teardown fails with a
/// stage name instead of wedging the suite).
fn runServe(a: RunArgs) void {
    serve.run(a.io, a.env_map, a.argv, a.alloc) catch |e| {
        a.run_err.* = e;
    };
    a.run_done.set();
}

test "integration: daemon boot -> SIGTERM -> drain runs the real teardown clean" {
    const alloc = testing.allocator;
    const io = common.globalIo();

    // Skip outside the dedicated isolated lane (see LIFECYCLE_ISOLATION_ENV): a
    // full real-daemon boot must not run interleaved with the shared suite.
    if (common.env.testLiveValue(LIFECYCLE_ISOLATION_ENV) == null) return error.SkipZigTest;

    // ── Env gate: build the daemon env from live infra ONLY. Never snapshot the
    // live environment — a dev machine's POSTHOG_API_KEY / OTLP endpoint would
    // leak in and spawn real exporters under the test.
    const db_url = common.env.testLiveValue(OS_DB_URL_ENV) orelse return error.SkipZigTest;
    const redis_url = common.env.testLiveValue(OS_REDIS_TLS_URL_ENV) orelse return error.SkipZigTest;
    const ca_cert = common.env.testLiveValue(OS_REDIS_CA_CERT_ENV) orelse return error.SkipZigTest;

    var env_map = try common.env.fromPairs(alloc, &.{
        .{ db.roleEnvVarName(.api), db_url },
        .{ REDIS_URL_API_ENV, redis_url },
        .{ REDIS_CA_CERT_ENV, ca_cert },
        .{ "OIDC_ISSUER", OIDC_ISSUER },
        .{ "OIDC_AUDIENCE", OIDC_AUDIENCE },
        .{ "OIDC_PROVIDER", OIDC_PROVIDER },
        .{ "ENCRYPTION_MASTER_KEY", ENCRYPTION_MASTER_KEY },
        .{ "AUTH_SESSION_CODE_PEPPER", SESSION_PEPPER },
        .{ "AUDIT_LOG_PEPPER", AUDIT_PEPPER },
    });
    defer env_map.deinit();

    // ── Probe Postgres + Redis: serve.run process.exit(1)s on an unreachable or
    // unmigrated db or an unreachable Redis, which would kill the WHOLE test
    // binary. Probing first converts every such boot failure into a clean skip.
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    // Release the auto-acquired conn so the pool always has a free slot for the
    // migration probe's own acquire and each seed/cleanup phase below.
    db_ctx.pool.release(db_ctx.conn);
    cmd_common.enforceServeMigrationSafety(io, &env_map, alloc, db_ctx.pool, false) catch return error.SkipZigTest;
    {
        var redis_probe = queue_redis.testing.connectFromUrl(io, alloc, redis_url) catch return error.SkipZigTest;
        redis_probe.deinit();
    }

    // ── Seed the tenant/workspace/fleet + agt_t API key the SSE stream
    // authenticates against.
    {
        const c = try db_ctx.pool.acquire();
        defer db_ctx.pool.release(c);
        try seedFixtures(c);
    }
    defer cleanupFixtures(db_ctx.pool);

    // ── Restore default signal disposition + clear the shutdown flag for later
    // tests: a lingering SIGTERM handler would swallow a Continuous Integration
    // (CI) timeout-kill; a stuck shutdown_requested=true would bleed into the
    // next serve_shutdown test.
    defer restoreDefaultSignals();
    defer serve_shutdown.reset();

    // ── Concurrency-capable Io for the daemon, mirroring the process Io that
    // main.zig hands serve.run. `common.globalIo()` is statically
    // single-threaded (`concurrent_limit = .nothing`), so the hub's raced
    // bounded dial (`std.Io.Select` in subscription_hub_wire.connectBounded)
    // would fail its very first `select.concurrent` with ConcurrencyUnavailable
    // and serve.run would exit(1) the whole test binary. Same fixture shape as
    // subscription_hub_test.TestScheduler and http/test_harness.
    //
    // Deinit is skipped once the daemon is `detach()`ed: `Threaded.deinit`
    // joins the pool and then poisons itself, so freeing it under a
    // still-running serve.run is a use-after-free. The failure paths below
    // detach a live daemon and bail loudly (by design — a wedged teardown must
    // fail the test, never hang the suite); on those we deliberately leak this
    // Io rather than free it out from under the live thread. Every non-detached
    // exit (clean shutdown, or all boot attempts joined after dying early) has
    // no live reference, so it deinits normally. Unlike the process-immortal
    // `globalIo()` it replaces, this Io owns joinable workers.
    var serve_io = std.Io.Threaded.init(alloc, .{});
    var daemon_detached = false;
    defer if (!daemon_detached) serve_io.deinit();

    // ── Boot the REAL serve.run on a thread, retrying on a fresh port if httpz
    // loses the allocFreePort bind race (run() returns before ever serving).
    var run_done: common.Event = .{};
    var run_err: ?anyerror = null;
    var port_buf: [PORT_ARG_BUF]u8 = undefined;
    var argv_store = [_][:0]const u8{ SERVE_ARGV0, SERVE_SUBCMD, SERVE_ARGV0 };

    var serve_thread: ?std.Thread = null;
    var served_port: u16 = 0;
    var attempt: usize = 0;
    while (attempt < MAX_BOOT_ATTEMPTS) : (attempt += 1) {
        const port = try test_port.allocFreePort();
        argv_store[2] = try std.fmt.bufPrintZ(&port_buf, "--port={d}", .{port});
        run_done = .{};
        run_err = null;
        const t = try std.Thread.spawn(.{}, runServe, .{RunArgs{
            .io = serve_io.io(),
            .env_map = &env_map,
            .argv = &argv_store,
            .alloc = alloc,
            .run_done = &run_done,
            .run_err = &run_err,
        }});
        switch (waitBoot(port, &run_done)) {
            .served => {
                serve_thread = t;
                served_port = port;
                break;
            },
            // run() returned before ever serving → bind race (or a returned
            // Server.init error); join the finished thread and retry a fresh port.
            .died_early => t.join(),
            // Boot is alive but never served: raising SIGTERM now could hit a
            // pre-handler window, so detach and fail loudly instead.
            .timed_out => {
                t.detach();
                daemon_detached = true;
                return error.DaemonBootTimedOut;
            },
        }
    }
    const thread = serve_thread orelse return error.DaemonBootRetriesExhausted;

    // ── Server proven listening → the handler is installed. Hold one
    // authenticated SSE stream open (its 200 means the stream is registered),
    // THEN raise SIGTERM through the real handler.
    const stream_path = try std.fmt.allocPrint(alloc, SSE_PATH_FMT, .{ WORKSPACE_ID, FLEET_ID });
    defer alloc.free(stream_path);
    var sse = SseClient.connect(alloc, served_port, stream_path, .{
        .bearer = AGT_T_KEY,
        .deadline_ms = SSE_DEADLINE_MS,
    }) catch |err| {
        // Boot succeeded but the stream didn't attach — detach so a failed
        // assertion never hangs the suite, then surface the connect error.
        thread.detach();
        daemon_detached = true;
        return err;
    };
    defer sse.deinit();

    // raise() == pthread_kill(self): the handler runs on this thread and sets
    // the shutdown flag the watcher polls.
    try std.posix.raise(std.posix.SIG.TERM);

    // The teardown's streams.drain() must shut this live stream's client fd: a
    // bounded read hits EOF. This is the freed-under-a-live-stream defect class
    // that an empty-registry test cannot exercise.
    try expectSseEof(&sse);

    // run() returned within the bound → every worker/sweeper/request thread
    // joined and all daemon state was freed.
    run_done.timedWait(SHUTDOWN_NS) catch {
        thread.detach();
        daemon_detached = true;
        return error.DaemonShutdownTimedOut;
    };
    thread.join();
    try testing.expect(run_err == null); // listen() returned cleanly, no boot error

    // The listener is torn down → a fresh /healthz connect no longer succeeds.
    try testing.expect(!healthzOk(served_port));

    // Run-proof marker (raw stderr, bypasses the log-sink fail-on-warn): the
    // memleak lane greps this to prove the boot→drain test actually EXECUTED
    // under valgrind rather than silently skipping on a misconfigured env — an
    // unmarked skip would make the P0 leak claim vacuous.
    std.debug.print("{s}\n", .{LIFECYCLE_RAN_MARKER});
}

// Greppable proof the lifecycle test ran to completion — the memleak lane
// greps this exact string from the valgrind run's stderr (RULE UFS: named once).
const LIFECYCLE_RAN_MARKER = "SERVE_LIFECYCLE_BOOT_DRAIN_RAN";

/// Poll the boot witness: `/healthz` 200 means listening + handler installed;
/// `run_done` set first means run() returned without serving (retry a port).
fn waitBoot(port: u16, run_done: *const common.Event) BootOutcome {
    var waited_ms: u32 = 0;
    while (waited_ms < BOOT_TIMEOUT_MS) : (waited_ms += HEALTHZ_POLL_MS) {
        if (run_done.isSet()) return .died_early;
        if (healthzOk(port)) return .served;
        common.sleepNanos(@as(u64, HEALTHZ_POLL_MS) * std.time.ns_per_ms);
    }
    return .timed_out;
}

/// One raw HTTP/1.1 GET /healthz — true only on a parsed 200. Doubles as the
/// post-shutdown liveness probe (a refused connect returns false).
fn healthzOk(port: u16) bool {
    const io = common.globalIo();
    var addr = std.Io.net.IpAddress.parseIp4("127.0.0.1", port) catch return false;
    const stream = addr.connect(io, .{ .mode = .stream }) catch return false;
    defer stream.close(io);
    setReadTimeout(stream.socket.handle, HEALTHZ_READ_MS);
    var wbuf: [HEALTHZ_REQ_BUF]u8 = undefined;
    var w = stream.writer(io, &wbuf);
    w.interface.writeAll(HEALTHZ_REQUEST) catch return false;
    w.interface.flush() catch return false;
    var rbuf: [HEALTHZ_RESP_BUF]u8 = undefined;
    const n = std.posix.read(stream.socket.handle, &rbuf) catch return false;
    return std.mem.indexOf(u8, rbuf[0..n], HTTP_200) != null;
}

/// Consume any trailing frames, then require EOF: the drain closed the socket.
/// A read timeout (or too many frames) means the drain never closed it — fail.
fn expectSseEof(sse: *SseClient) !void {
    var i: usize = 0;
    while (i < EOF_MAX_FRAMES) : (i += 1) {
        var frame = sse.nextFrame() catch |e| switch (e) {
            error.EndOfStream => return, // drain shut the fd — the assertion
            else => return e, // SseFrameTimeout etc → the drain never closed it
        };
        frame.deinit(sse.alloc);
    }
    return error.SseStreamNeverClosed;
}

fn setReadTimeout(fd: std.posix.fd_t, ms: u32) void {
    const timeout = std.posix.timeval{
        .sec = @intCast(ms / MS_PER_SECOND),
        .usec = @intCast((ms % MS_PER_SECOND) * US_PER_MS),
    };
    std.posix.setsockopt(fd, std.posix.SOL.SOCKET, std.posix.SO.RCVTIMEO, std.mem.asBytes(&timeout)) catch |err|
        std.log.warn(IGNORED_ERR_FMT, .{@errorName(err)});
}

fn restoreDefaultSignals() void {
    const dfl = std.posix.Sigaction{
        .handler = .{ .handler = std.posix.SIG.DFL },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &dfl, null);
    std.posix.sigaction(std.posix.SIG.TERM, &dfl, null);
}

fn seedFixtures(conn: *pg.Conn) !void {
    const now_ms = common.clock.nowMillis();
    try base.seedTenantById(conn, TENANT_ID, TENANT_NAME);
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, FLEET_NAME, FLEET_CONFIG_JSON, FLEET_SOURCE_MD);
    const key_hash = api_key.sha256Hex(AGT_T_KEY);
    _ = try conn.exec(
        \\INSERT INTO core.api_keys (uid, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'lifecycle-test-key', '', $3::text, 'user_lifecycle_test', TRUE, $4::bigint, $4::bigint)
        \\ON CONFLICT (key_hash) DO NOTHING
    , .{ API_KEY_ROW_ID, TENANT_ID, key_hash[0..], now_ms });
}

fn cleanupFixtures(pool: *pg.Pool) void {
    const conn = pool.acquire() catch return;
    defer pool.release(conn);
    base.teardownFleets(conn, WORKSPACE_ID);
    _ = conn.exec("DELETE FROM core.api_keys WHERE uid = $1::uuid", .{API_KEY_ROW_ID}) catch |err|
        std.log.warn(IGNORED_ERR_FMT, .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.workspaces WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err|
        std.log.warn(IGNORED_ERR_FMT, .{@errorName(err)});
    base.teardownTenantById(conn, TENANT_ID);
}
