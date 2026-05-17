//! Slice-1 smoke tests for `Pool` + `Connection`.
//!
//! Exercises every public surface against an in-process RESP fake so the
//! tests are deterministic across CI environments (no Redis needed). The
//! fake speaks just enough RESP for one PING reply, then drops — that is
//! enough to verify the dial / read-one-reply / poison / release paths.
//! Slice 9 layers full integration tests on top of this.

const std = @import("std");
const Pool = @import("redis_pool.zig");
const Connection = @import("redis_connection.zig");
const redis_config = @import("redis_config.zig");

// Loopback host shared across every in-process fake server below.
// `parseIp4` (for `listen`) and `alloc.dupe` (for `Config.host`) both want
// the same literal — extracting prevents drift.
const LOOPBACK_IPV4 = "127.0.0.1";

// Shutdown-loop poll tick for fake servers. Pre-existing sites in
// `PingFake` / `CloseOnCmd` already use 10ms; we name the value so that
// new fakes don't drift to 5ms or 100ms by copy-paste.
const FAKE_SHUTDOWN_POLL_NS = 10 * std.time.ns_per_ms;

const PingFake = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),
    keep_open: bool,

    fn start(self: *PingFake, keep_open: bool) !void {
        const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.keep_open = keep_open;
        self.thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *PingFake) void {
        self.stop.store(true, .release);
        const addr = self.addr;
        if (std.net.tcpConnectToAddress(addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn acceptLoop(self: *PingFake) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept() catch return;
            if (self.stop.load(.acquire)) {
                conn.stream.close();
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);

            var buf: [256]u8 = undefined;
            const n = conn.stream.read(&buf) catch 0;
            if (n > 0) conn.stream.writeAll("+PONG\r\n") catch {};
            if (self.keep_open) {
                // Drift until shutdown — leaves the socket usable for a
                // second command, which we don't issue. We just need the
                // fd to remain open so release(ok=true) can park the conn.
                while (!self.stop.load(.acquire)) std.Thread.sleep(10 * std.time.ns_per_ms);
            }
            conn.stream.close();
        }
    }

    fn config(self: *PingFake, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, "127.0.0.1");
        return .{
            .host = host,
            .port = self.addr.in.getPort(),
            .use_tls = false,
        };
    }
};

test "Pool.init with eager_min=0 dials nothing; deinit is a no-op" {
    const alloc = std.testing.allocator;

    const host = try alloc.dupe(u8, "127.0.0.1");
    const cfg: redis_config.Config = .{ .host = host, .port = 1, .use_tls = false };

    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 4, .eager_min = 0 });
    defer pool.deinit();

    const s = pool.stats();
    try std.testing.expectEqual(@as(usize, 0), s.idle);
    try std.testing.expectEqual(@as(usize, 0), s.active);
    try std.testing.expectEqual(@as(u64, 0), s.dials_total);
}

test "Pool.init with eager_min=N preconnects N connections" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 4, .eager_min = 2 });
    defer pool.deinit();

    const s = pool.stats();
    try std.testing.expectEqual(@as(usize, 2), s.idle);
    try std.testing.expectEqual(@as(u64, 2), s.dials_total);
}

test "Pool.acquire reuses idle, release(ok=true) parks back" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 4, .eager_min = 1 });
    defer pool.deinit();

    const conn = try pool.acquire();
    try std.testing.expectEqual(Connection.Role.pooled, conn.role);

    try std.testing.expectEqual(@as(usize, 0), pool.stats().idle);
    try std.testing.expectEqual(@as(usize, 1), pool.stats().active);

    pool.release(conn, true);
    try std.testing.expectEqual(@as(usize, 1), pool.stats().idle);
    try std.testing.expectEqual(@as(usize, 0), pool.stats().active);
}

test "Pool.release(ok=false) closes the connection" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 4, .eager_min = 1 });
    defer pool.deinit();

    const conn = try pool.acquire();
    pool.release(conn, false);

    const s = pool.stats();
    try std.testing.expectEqual(@as(usize, 0), s.idle);
    try std.testing.expectEqual(@as(usize, 0), s.active);
}

test "Pool.release(ok=true) over max_idle increments forced_closes_total" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 1, .eager_min = 0 });
    defer pool.deinit();

    // Two concurrent acquires push active above max_idle: first release
    // parks back to idle (within cap); second release exceeds the cap and
    // must force-close. Counter discriminates "stayed within cap" from
    // "burst-discard happened" in operator scrapes of /metrics.
    const a = try pool.acquire();
    const b = try pool.acquire();

    pool.release(a, true);
    try std.testing.expectEqual(@as(u64, 0), pool.stats().forced_closes_total);
    try std.testing.expectEqual(@as(usize, 1), pool.stats().idle);

    pool.release(b, true);
    try std.testing.expectEqual(@as(u64, 1), pool.stats().forced_closes_total);
    try std.testing.expectEqual(@as(usize, 1), pool.stats().idle);
}

test "Pool.release 16-cycle stress at max_idle=8 holds idle==8 steady-state with extras force-closed" {
    // Spec §pool-sizing: max_idle is a hard cap on parked conns, not a hint.
    // Acquire 16 conns concurrently (drives active > max_idle), then release
    // all 16. Exactly the first 8 land in idle; the trailing 8 force-close
    // and bump forced_closes_total. After the burst, every subsequent
    // acquire/release cycle reuses an idle conn — no further force-closes,
    // dials_total stays at 16. Catches a regression where max_idle is
    // treated as soft (off-by-one in the cap check) or where the force-close
    // path leaks the Connection allocation back into idle.
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 8, .eager_min = 0 });
    defer pool.deinit();

    var conns: [16]*@import("redis_connection.zig") = undefined;
    for (&conns) |*slot| slot.* = try pool.acquire();

    // 16 concurrent acquires: 16 fresh dials, 16 active, 0 idle.
    try std.testing.expectEqual(@as(usize, 16), pool.stats().active);
    try std.testing.expectEqual(@as(usize, 0), pool.stats().idle);
    try std.testing.expectEqual(@as(u64, 16), pool.stats().dials_total);

    // Release all 16 ok=true: first 8 park (idle climbs to 8); trailing 8
    // exceed the cap and force-close. forced_closes_total bumps by 8.
    for (conns) |c| pool.release(c, true);

    const after_burst = pool.stats();
    try std.testing.expectEqual(@as(usize, 8), after_burst.idle);
    try std.testing.expectEqual(@as(usize, 0), after_burst.active);
    try std.testing.expectEqual(@as(u64, 8), after_burst.forced_closes_total);

    // Steady-state: 16 single acquire/release cycles must reuse idle conns
    // unchanged. No new dials, no new force-closes — `dials_total` and
    // `forced_closes_total` are frozen at their burst values; `idle` stays
    // at the cap.
    var i: usize = 0;
    while (i < 16) : (i += 1) {
        const c = try pool.acquire();
        pool.release(c, true);
    }

    const steady = pool.stats();
    try std.testing.expectEqual(@as(usize, 8), steady.idle);
    try std.testing.expectEqual(@as(usize, 0), steady.active);
    try std.testing.expectEqual(@as(u64, 16), steady.dials_total);
    try std.testing.expectEqual(@as(u64, 8), steady.forced_closes_total);
}

test "Pool.acquire OOM during create does not leak active_count" {
    // Greptile Issue 2 fix witness: the dial-failure errdefer must decrement
    // active_count for every failure path, not only the catch-block one. A
    // pure OOM during `alloc.create(Connection)` is the cheapest, most
    // deterministic trip — it doesn't depend on syscall ordering or network
    // state. FailingAllocator(fail_index=0) ensures the very first
    // post-init alloc fails, which IS the Connection create on line 133.
    const alloc = std.testing.allocator;

    const host = try alloc.dupe(u8, "127.0.0.1");
    const cfg: redis_config.Config = .{ .host = host, .port = 1, .use_tls = false };

    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 1, .eager_min = 0 });
    // Restore the real allocator before deinit so cfg.host frees through the
    // same allocator it was duped with.
    defer {
        pool.alloc = std.testing.allocator;
        pool.deinit();
    }

    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    pool.alloc = fa.allocator();

    const result = pool.acquire();
    try std.testing.expectError(error.OutOfMemory, result);

    // Load-bearing assertion: the errdefer at acquire() line 127 must have
    // fired and rolled active_count back to 0. Without the fix, this would
    // remain at 1 (the increment before the dial) and overflow_dials_total
    // logic would treat the pool as one slot more saturated than reality.
    try std.testing.expectEqual(@as(usize, 0), pool.stats().active);
}

test "Pool.release of poisoned conn increments poisoned counter" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 4, .eager_min = 1 });
    defer pool.deinit();

    const conn = try pool.acquire();
    // Drive the conn to .poisoned via the transitionTo path command() takes
    // on IO error, without needing a real failure: we mark it directly through
    // the same legal transition table command() uses.
    conn.state = .poisoned;
    pool.release(conn, true); // ok=true with poisoned still closes
    try std.testing.expectEqual(@as(u64, 1), pool.stats().poisoned_connections_total);
}

// Dedicated / subscriber role + command round-trip live as in-file tests
// inside redis_connection.zig — those exercise file-private symbols (init,
// command). This file keeps only Pool-surface coverage.

test "Pool.stats surfaces every PoolStats field" {
    // Compile-time witness: every documented field is read. Keeps slice-7
    // /metrics wiring honest about which counters exist.
    const alloc = std.testing.allocator;
    const host = try alloc.dupe(u8, "127.0.0.1");
    const cfg: redis_config.Config = .{ .host = host, .port = 1, .use_tls = false };
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 1, .eager_min = 0 });
    defer pool.deinit();

    const s = pool.stats();
    _ = s.active;
    _ = s.idle;
    _ = s.dials_total;
    _ = s.overflow_dials_total;
    _ = s.acquire_wait_ns_p99;
    _ = s.poisoned_connections_total;
    _ = s.reconnects_total;
    _ = s.forced_closes_total;
    _ = s.acquire_timeouts_total;
}

// ── Integration tests (real Redis via TEST_REDIS_TLS_URL) ───────────────
//
// Pattern lifted from `redis_test.zig` "integration: rediss ping" — skip
// when the env var is unset so unit-test runs in CI / dev stay deterministic
// without a live broker. Exercises behaviors that only a real RESP server
// can prove: PING round-trip through the pool, over-`max_idle` dial path,
// and `SO_RCVTIMEO`-triggered `ReadFailed` surfacing after MAX_ATTEMPTS.

const TLS_URL_ENV = "TEST_REDIS_TLS_URL";
const REDISS_SCHEME = "rediss://";

fn tlsUrlOrSkip(alloc: std.mem.Allocator) ![]u8 {
    const url = std.process.getEnvVarOwned(alloc, TLS_URL_ENV) catch return error.SkipZigTest;
    if (!std.mem.startsWith(u8, url, REDISS_SCHEME)) {
        alloc.free(url);
        return error.SkipZigTest;
    }
    return url;
}

test "integration: pool acquires, parks idle, and over-cap dial increments overflow_dials_total" {
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    const cfg = try redis_config.parseRedisUrl(alloc, tls_url);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 2, .eager_min = 1 });
    defer pool.deinit();

    // Happy path: one acquire, one real PING round-trip, release(ok=true).
    // Confirms RESP travels end-to-end through the pool against a live server.
    const first = try pool.acquire();
    var pong = try first.command(&.{"PING"});
    pong.deinit(alloc);
    pool.release(first, true);
    try std.testing.expectEqual(@as(usize, 1), pool.stats().idle);

    // Burst beyond `max_idle = 2`: hold 4 connections simultaneously. Acquires
    // 3 and 4 see `active_count >= max_idle` at the dial decision point, so
    // each increments `overflow_dials_total`. The pool grows transparently
    // without an exhaustion error (no hard cap; over-`max_idle` connections
    // close on release rather than parking back into idle).
    var held: [4]*Connection = undefined;
    for (&held) |*slot| slot.* = try pool.acquire();

    const peak = pool.stats();
    try std.testing.expectEqual(@as(usize, 0), peak.idle);
    try std.testing.expectEqual(@as(usize, 4), peak.active);
    try std.testing.expect(peak.overflow_dials_total >= 2);

    for (held) |c| pool.release(c, true);

    // After release: idle is capped at `max_idle = 2`; the other 2 closed.
    const settled = pool.stats();
    try std.testing.expectEqual(@as(usize, 2), settled.idle);
    try std.testing.expectEqual(@as(usize, 0), settled.active);
}

test "integration: client.command on real WRONGTYPE reply recycles conn (no reconnect)" {
    // Real-Redis sibling for the in-process #6 unit test. Proves the
    // resumable-error path against a real RESP `.err` reply (server-side
    // WRONGTYPE on LPUSH-into-string). The unit test uses hand-crafted
    // `-ERR fake_error\r\n`; real-server error strings carry additional
    // detail and have caused parser-edge regressions before.
    //
    // Flow: SET key="<unique>" "v" → +OK; LPUSH same key "x" → WRONGTYPE
    // → RedisCommandError; DEL key → :1. The DEL succeeds on the SAME
    // pooled conn that the WRONGTYPE was returned on — proving the conn
    // was parked back in idle and reused. `reconnects_total` stays at 0
    // (resumable doesn't redial).
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    var client = try Client.connectFromUrlWithOptions(alloc, tls_url, .{ .read_timeout_ms = 5000 });
    defer client.deinit();

    // Unique key per run keeps tests state-isolated across parallel CI runs.
    var key_buf: [64]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "uz:m69:wrongtype:{d}", .{std.time.nanoTimestamp()});

    var set_resp = try client.command(&.{ "SET", key, "v" });
    set_resp.deinit(alloc);

    const wrongtype = client.command(&.{ "LPUSH", key, "x" });
    try std.testing.expectError(error.RedisCommandError, wrongtype);

    try std.testing.expectEqual(@as(u64, 0), client.pool.stats().reconnects_total);

    var del_resp = try client.command(&.{ "DEL", key });
    defer del_resp.deinit(alloc);
    try std.testing.expect(del_resp == .integer);
    try std.testing.expectEqual(@as(i64, 1), del_resp.integer);

    try std.testing.expectEqual(@as(u64, 0), client.pool.stats().reconnects_total);
}

test "integration: request timeout surfaces ReadFailed on a hanging BLPOP" {
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    // `read_timeout_ms = 100` arms `SO_RCVTIMEO` on every pooled Connection.
    // BLPOP with a 5s server-side wait on a never-populated key holds the
    // socket idle past the budget; the kernel returns EAGAIN, std.Io.Reader
    // surfaces `error.ReadFailed`, and `Connection.mapReadError` keeps it
    // as `error.ReadFailed` (no separate timeout variant — std.Io.Reader
    // doesn't expose the underlying errno so we'd misattribute peer-side
    // drops as timeouts). Pool retry dials a fresh conn for attempt 2 —
    // it also times out — so the error surfaces after MAX_ATTEMPTS.
    var client = try Client.connectFromUrlWithOptions(alloc, tls_url, .{ .read_timeout_ms = 100 });
    defer client.deinit();

    const start = std.time.nanoTimestamp();
    const result = client.command(&.{ "BLPOP", "test:blpop:never-populated", "5" });
    const elapsed_ns = std.time.nanoTimestamp() - start;

    try std.testing.expectError(error.ReadFailed, result);
    // Sanity bound: two ~100ms timeouts + one redial fit well under 5s. If
    // this assertion ever trips, the retry loop is hanging — not just slow.
    try std.testing.expect(elapsed_ns < 5 * std.time.ns_per_s);
}

// Closes the socket after reading the first command. Drives a non-resumable
// transport error (`error.ReadFailed` via `mapReadError`) into the Client's
// retry loop — exercises greptile Issue 3 (reconnects_total never incremented).
const CloseOnCmd = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *CloseOnCmd) !void {
        const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *CloseOnCmd) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *CloseOnCmd) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept() catch return;
            if (self.stop.load(.acquire)) {
                conn.stream.close();
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);
            // Swallow whatever the client wrote (a RESP-encoded PING), then
            // close. The client's response read returns EOF → ReadFailed →
            // non-resumable. The Pool retry layer redials a fresh conn for
            // attempt 2; this loop accepts that too and closes the same way.
            var buf: [256]u8 = undefined;
            _ = conn.stream.read(&buf) catch {};
            conn.stream.close();
        }
    }

    fn config(self: *CloseOnCmd, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, "127.0.0.1");
        return .{ .host = host, .port = self.addr.in.getPort(), .use_tls = false };
    }
};

// Serves `-ERR fake_error\r\n` on the first read, `+PONG\r\n` on the second,
// then drifts until shutdown — same TCP socket. Lets us prove that a resumable
// `.err` reply leaves the connection parked back in idle and the caller's
// follow-up `acquire` reuses it (no redial, no recordReconnect).
const ErrThenPongOnSameSocket = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *ErrThenPongOnSameSocket) !void {
        const loopback = try std.net.Address.parseIp4(LOOPBACK_IPV4, 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *ErrThenPongOnSameSocket) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *ErrThenPongOnSameSocket) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept() catch return;
            if (self.stop.load(.acquire)) {
                conn.stream.close();
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);

            var buf: [256]u8 = undefined;
            // Cmd 1 → -ERR (resumable: protocol frame is whole).
            const n1 = conn.stream.read(&buf) catch 0;
            // pin test: literal is the contract — RESP `-ERR <msg>\r\n`
            // is the resumable error frame the Client retry layer
            // classifies via `isResumable`. The exact message text isn't
            // load-bearing, but the leading `-` and trailing CRLF are.
            if (n1 > 0) conn.stream.writeAll("-ERR fake_error\r\n") catch {};
            // Cmd 2 → +PONG on the SAME socket. Blocks until the test
            // issues its follow-up command after release(ok=true).
            const n2 = conn.stream.read(&buf) catch 0;
            // pin test: literal is the contract — RESP simple-string OK
            // reply. Changing the prefix breaks the protocol parser, not
            // just this test.
            if (n2 > 0) conn.stream.writeAll("+PONG\r\n") catch {};
            while (!self.stop.load(.acquire)) std.Thread.sleep(FAKE_SHUTDOWN_POLL_NS);
            conn.stream.close();
        }
    }

    fn config(self: *ErrThenPongOnSameSocket, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, LOOPBACK_IPV4);
        return .{ .host = host, .port = self.addr.in.getPort(), .use_tls = false };
    }
};

test "Client.command on resumable .err reply parks conn back; follow-up reuses same socket" {
    // Spec assertion: resumable server-side errors (the `.err` RespValue
    // path) release `ok=true` so the connection stays in idle and the
    // caller's retry uses the same conn — no extra dial, no recordReconnect.
    // `accepts == 1` and `dials_total == 1` prove the same TCP socket
    // served both cycles; `reconnects_total == 0` proves the retry layer
    // didn't treat this as transport churn.
    const alloc = std.testing.allocator;

    var fake: ErrThenPongOnSameSocket = undefined;
    try fake.start();
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    const pool = try Pool.init(alloc, cfg, .{ .max_idle = 2, .eager_min = 0 });
    var client: Client = .{ .alloc = alloc, .pool = pool };
    defer client.deinit();

    // Cycle 1: fresh dial, write PING, read -ERR. Client.command returns
    // RedisCommandError (resumable → release(ok=true)); conn parks in idle.
    const r1 = client.command(&.{"PING"});
    try std.testing.expectError(error.RedisCommandError, r1);

    const after_err = client.pool.stats();
    try std.testing.expectEqual(@as(u64, 0), after_err.reconnects_total);
    try std.testing.expectEqual(@as(u64, 1), after_err.dials_total);
    try std.testing.expectEqual(@as(usize, 1), after_err.idle);
    try std.testing.expectEqual(@as(usize, 0), after_err.active);

    // Cycle 2: caller retries. acquire pops the same conn (idle.popFirst).
    // Same fake accept → same socket → +PONG on the second read.
    var r2 = try client.command(&.{"PING"});
    defer r2.deinit(alloc);
    try std.testing.expect(r2 == .simple);
    try std.testing.expectEqualStrings("PONG", r2.simple);

    const after_pong = client.pool.stats();
    try std.testing.expectEqual(@as(u64, 0), after_pong.reconnects_total);
    try std.testing.expectEqual(@as(u64, 1), after_pong.dials_total); // no redial
    try std.testing.expectEqual(@as(u32, 1), fake.accepts.load(.monotonic));
}

test "Client.command bumps reconnects_total on non-resumable transport error" {
    // Greptile Issue 3 fix witness: a non-resumable transport failure inside
    // the retry loop must increment reconnects_total before the redial.
    // Without recordReconnect(), the metric stays at 0 even though the
    // client did dial fresh — operator dashboards lose visibility into
    // transient transport churn.
    const alloc = std.testing.allocator;

    var fake: CloseOnCmd = undefined;
    try fake.start();
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    const pool = try Pool.init(alloc, cfg, .{ .max_idle = 2, .eager_min = 0 });
    var client: Client = .{ .alloc = alloc, .pool = pool };
    defer client.deinit();

    try std.testing.expectEqual(@as(u64, 0), client.pool.stats().reconnects_total);

    // Attempt 1: dial fresh (no idle conns), write PING, fake closes →
    //   ReadFailed (non-resumable) → release(conn, false) → recordReconnect → retry.
    // Attempt 2: dial fresh again, same failure. attempt+1 >= MAX_ATTEMPTS=2,
    //   so the error surfaces without a third recordReconnect.
    const result = client.command(&.{"PING"});
    try std.testing.expectError(error.ReadFailed, result);

    // Exactly one recordReconnect: the bump between attempt 1's failure
    // and attempt 2's redial. MAX_ATTEMPTS=2 forbids a third try.
    try std.testing.expectEqual(@as(u64, 1), client.pool.stats().reconnects_total);

    // Fresh-dial witness: attempt 2's `acquire` redialed a NEW TCP socket
    // (the poisoned attempt-1 conn was closed in release(ok=false)). The
    // fake accepted >=2 connections — one per attempt. A pool that
    // mistakenly recycled the poisoned conn would have accepts=1 and the
    // second attempt would block waiting for a reply on the dead socket.
    try std.testing.expect(fake.accepts.load(.monotonic) >= 2);
    // Pool-side accounting matches: dials_total tracks both attempts.
    try std.testing.expectEqual(@as(u64, 2), client.pool.stats().dials_total);
}

const Client = @import("redis_client.zig");
const metrics_redis_pool = @import("../observability/metrics_redis_pool.zig");
const metrics_render = @import("../observability/metrics_render.zig");

test "metrics: registerPool + activity renders all 8 zombie_redis_pool_* lines in Prometheus output" {
    // Spec coverage: prove the scrape path emits a line per `PoolStats`
    // field after a Pool is registered. The render layer treats `mrp`
    // as the single source of truth — a regression that drops a counter
    // from the renderer would show up here as a missing metric name.
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start(true);
    defer fake.shutdown();

    const cfg = try fake.config(alloc);
    var pool = try Pool.init(alloc, cfg, .{ .max_idle = 2, .eager_min = 0 });
    defer pool.deinit();

    // Register on the singleton; clear on the way out so subsequent tests
    // see `snapshot() == null`. Without the deferred clear, a parallel
    // test scraping /metrics would observe this Pool's stale stats.
    metrics_redis_pool.registerPool(&pool);
    defer metrics_redis_pool.clearRegisteredPool();

    // Drive: 3 acquire/command/release cycles → dials_total=1, idle=1
    // (first acquire dials; subsequent reuse). Force one poisoned release
    // to bump poisoned_connections_total via release(ok=false).
    inline for (0..3) |_| {
        const c = try pool.acquire();
        var resp = try c.command(&.{"PING"});
        resp.deinit(alloc);
        pool.release(c, true);
    }
    const poison_conn = try pool.acquire();
    pool.release(poison_conn, false);

    const text = try metrics_render.renderPrometheus(alloc, true);
    defer alloc.free(text);

    // Every Pool-stats line that ships in metrics_render.zig must appear.
    // Match the metric NAMES (not values) — values drift with timing and
    // the renderer's order; names are the contract scrapers depend on.
    const expected_metric_names = [_][]const u8{
        "zombie_redis_pool_active",
        "zombie_redis_pool_idle",
        "zombie_redis_pool_dials_total",
        "zombie_redis_pool_overflow_dials_total",
        "zombie_redis_pool_poisoned_connections_total",
        "zombie_redis_pool_reconnects_total",
        "zombie_redis_pool_forced_closes_total",
        "zombie_redis_pool_acquire_timeouts_total",
    };
    inline for (expected_metric_names) |name| {
        if (std.mem.indexOf(u8, text, name) == null) {
            std.debug.print("FAIL: metric `{s}` missing from rendered Prometheus output\n", .{name});
            return error.MetricNameMissing;
        }
    }

    // Load-bearing value assertions: prove the snapshot path actually
    // pulls live stats (not zeroed defaults). 4 successful dials would
    // hit the cap of 2 in idle; the 4th acquire forced an over-cap dial.
    try std.testing.expect(std.mem.indexOf(u8, text, "zombie_redis_pool_dials_total 4") != null);
    try std.testing.expect(std.mem.indexOf(u8, text, "zombie_redis_pool_poisoned_connections_total 1") != null);
}
