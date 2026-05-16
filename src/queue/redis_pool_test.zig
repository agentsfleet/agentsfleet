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
// and `SO_RCVTIMEO` re-tag of `ReadFailed` into `RedisRequestTimeout`.

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

test "integration: request timeout surfaces RedisRequestTimeout on a hanging BLPOP" {
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    // `read_timeout_ms = 100` arms `SO_RCVTIMEO` on every pooled Connection.
    // BLPOP with a 5s server-side wait on a never-populated key holds the
    // socket idle past the budget; the kernel returns EAGAIN, std.Io.Reader
    // surfaces `error.ReadFailed`, and `Connection.mapReadError` re-tags it
    // as `error.RedisRequestTimeout` because `read_timeout_ms != null`.
    // Pool retry dials a fresh conn for attempt 2 — it also times out — so
    // the error surfaces to the caller after MAX_ATTEMPTS.
    var client = try Client.connectFromUrlWithOptions(alloc, tls_url, .{ .read_timeout_ms = 100 });
    defer client.deinit();

    const start = std.time.nanoTimestamp();
    const result = client.command(&.{ "BLPOP", "test:blpop:never-populated", "5" });
    const elapsed_ns = std.time.nanoTimestamp() - start;

    try std.testing.expectError(error.RedisRequestTimeout, result);
    // Sanity bound: two ~100ms timeouts + one redial fit well under 5s. If
    // this assertion ever trips, the retry loop is hanging — not just slow.
    try std.testing.expect(elapsed_ns < 5 * std.time.ns_per_s);
}

const Client = @import("redis_client.zig");
