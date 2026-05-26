const std = @import("std");
const redis = @import("redis.zig");

test "parseRedisUrl handles redis URL variants" {
    const alloc = std.testing.allocator;

    const local = try redis.testing.parseRedisUrl(alloc, "redis://localhost:6379");
    defer redis.testing.deinitConfig(alloc, local);
    try std.testing.expectEqualStrings("localhost", local.host);
    try std.testing.expectEqual(@as(u16, 6379), local.port);
    try std.testing.expect(!local.use_tls);

    const auth = try redis.testing.parseRedisUrl(alloc, "redis://user:pass@cache.local:6380");
    defer redis.testing.deinitConfig(alloc, auth);
    try std.testing.expectEqualStrings("cache.local", auth.host);
    try std.testing.expectEqualStrings("user", auth.username.?);
    try std.testing.expectEqualStrings("pass", auth.password.?);

    const tls = try redis.testing.parseRedisUrl(alloc, "rediss://default:secret@upstash.local:6379");
    defer redis.testing.deinitConfig(alloc, tls);
    try std.testing.expect(tls.use_tls);
}

test "parseRedisUrl supports rediss default port" {
    const alloc = std.testing.allocator;
    const tls = try redis.testing.parseRedisUrl(alloc, "rediss://localhost");
    defer redis.testing.deinitConfig(alloc, tls);
    try std.testing.expectEqualStrings("localhost", tls.host);
    try std.testing.expectEqual(@as(u16, 6379), tls.port);
    try std.testing.expect(tls.use_tls);
}

test "parseRedisUrl rejects invalid scheme and invalid port" {
    const alloc = std.testing.allocator;
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "http://localhost:6379"));
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "redis://localhost:notaport"));
    try std.testing.expectError(error.InvalidRedisUrl, redis.testing.parseRedisUrl(alloc, "redis://"));
}

test "loadCaBundle rejects relative custom CA path" {
    try std.testing.expectError(error.RedisTlsCaFileMustBeAbsolute, redis.testing.loadCaBundle(std.testing.allocator, "docker/redis/tls/ca.crt"));
}

test "integration: rediss ping via TEST_REDIS_TLS_URL" {
    const alloc = std.testing.allocator;
    const tls_url = std.process.getEnvVarOwned(alloc, "TEST_REDIS_TLS_URL") catch return error.SkipZigTest;
    defer alloc.free(tls_url);

    if (!std.mem.startsWith(u8, tls_url, "rediss://")) return error.SkipZigTest;

    var client = try redis.testing.connectFromUrl(alloc, tls_url);
    defer client.deinit();
    try client.ping();
}

test "roleEnvVarName maps redis roles deterministically" {
    try std.testing.expectEqualStrings("REDIS_URL", redis.roleEnvVarName(.default));
    try std.testing.expectEqualStrings("REDIS_URL_API", redis.roleEnvVarName(.api));
    try std.testing.expectEqualStrings("REDIS_URL_WORKER", redis.roleEnvVarName(.worker));
}

// ── Queue constants regression tests ─────────────────────────────────────

const queue_consts = @import("constants.zig");

test "queue constants: consumer prefix is stable" {
    try std.testing.expectEqualStrings("worker", queue_consts.consumer_prefix);
}


test "queue constants: XAUTOCLAIM cursor seed and batch size" {
    try std.testing.expectEqualStrings("0-0", queue_consts.xautoclaim_start);
    try std.testing.expectEqualStrings("1", queue_consts.xautoclaim_count);
}

// ── makeConsumerId tests ─────────────────────────────────────────────────

test "makeConsumerId starts with consumer prefix" {
    const alloc = std.testing.allocator;
    const id = try redis.makeConsumerId(alloc);
    defer alloc.free(id);
    try std.testing.expect(std.mem.startsWith(u8, id, queue_consts.consumer_prefix ++ "-"));
}

test "makeConsumerId produces unique IDs across calls" {
    const alloc = std.testing.allocator;
    const id1 = try redis.makeConsumerId(alloc);
    defer alloc.free(id1);
    const id2 = try redis.makeConsumerId(alloc);
    defer alloc.free(id2);
    try std.testing.expect(!std.mem.eql(u8, id1, id2));
}

// ── Keepalive + reconnect tests ──────────────────────────────────────────
// These exercise the patch that turned a long-lived idle connection from
// "silently dead" into "self-healing on next request" — the failure mode
// the prior dev `/readyz` `queue:false` regression uncovered. We use a
// tiny in-process RESP fake instead of a real Redis so the behaviour is
// deterministic across CI environments.

const redis_transport = @import("redis_transport.zig");

/// Fake Redis server: each accepted connection eats whatever the client
/// sends, replies `+PONG\r\n`, and closes. The reset on the second client
/// call is what proves the reconnect path works.
///
/// Per-connection handler threads: the pool dials `eager_min` connections
/// in parallel from one process, so a single-threaded accept-then-read
/// loop deadlocks (iter1 blocks reading conn-A; conn-B sits in the TCP
/// backlog; if the first command lands on conn-B, neither side advances).
/// Spawning a detached handler per accept decouples the conns at the
/// cost of one short-lived thread per dial — fine for a test fake.
const PingFake = struct {
    server: std.net.Server,
    addr: std.net.Address,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *PingFake) !void {
        const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *PingFake) void {
        self.stop.store(true, .release);
        // Wake the accept() by dialing ourselves; ignore failure (the
        // listener may already be torn down by the loop on error paths).
        const addr = self.addr;
        if (std.net.tcpConnectToAddress(addr)) |s| s.close() else |_| {}
        self.accept_thread.join();
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
            const t = std.Thread.spawn(.{}, handleConn, .{conn.stream}) catch {
                conn.stream.close();
                continue;
            };
            t.detach();
        }
    }

    fn handleConn(stream: std.net.Stream) void {
        defer stream.close();
        var buf: [256]u8 = undefined;
        const n = stream.read(&buf) catch 0;
        if (n == 0) return;
        stream.writeAll("+PONG\r\n") catch {};
        // Drop the connection — emulates Upstash idle-drop / restart.
    }

    fn url(self: *PingFake, alloc: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.addr.in.getPort()});
    }
};

test "PlainTransport.init enables SO_KEEPALIVE on its socket" {
    // Bind a loopback listener and dial it through PlainTransport so the
    // keepalive bit is exercised through the same path the real client uses.
    // SO_KEEPALIVE on the underlying fd is observable via getsockopt; the
    // per-OS keep-idle knobs are best-effort and not asserted.
    const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
    var listener = try loopback.listen(.{ .reuse_address = true });
    defer listener.deinit();

    const stream = try std.net.tcpConnectToAddress(listener.listen_address);
    var t = try redis_transport.PlainTransport.init(std.testing.allocator, stream);
    defer t.deinit(std.testing.allocator);

    var enabled: c_int = 0;
    try std.posix.getsockopt(
        t.stream.handle,
        std.posix.SOL.SOCKET,
        std.posix.SO.KEEPALIVE,
        std.mem.asBytes(&enabled),
    );
    try std.testing.expect(enabled != 0);
}

test "PlainTransport.init closes the stream on allocation failure (no fd leak)" {
    // Regression for the dialAndAuth fd-leak path: every reconnect calls
    // PlainTransport.init / TlsTransport.initInPlace; if that fails after
    // tcpConnectToHost succeeds, the socket must be closed or a sustained
    // period of init failures (TLS rotation, OOM) exhausts the fd table.
    // We force the failure by giving init a failing allocator and assert
    // that the second alloc call (write_buffer) gets a free()'d read_buffer
    // without the test allocator complaining about the leaked stream.
    const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
    var listener = try loopback.listen(.{ .reuse_address = true });
    defer listener.deinit();

    const stream = try std.net.tcpConnectToAddress(listener.listen_address);
    const fd_before = stream.handle;

    // FailingAllocator that errors on the 2nd allocation — read_buffer
    // succeeds (so init progresses past the first errdefer), write_buffer
    // fails (so init returns an error and our errdefer stream.close() must fire).
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 1 });
    const result = redis_transport.PlainTransport.init(failing.allocator(), stream);
    try std.testing.expectError(error.OutOfMemory, result);

    // Confirm the fd is no longer open. read() on a closed fd returns
    // EBADF, which Zig maps to error.NotOpenForReading — unlike fcntl
    // which panics on EBADF in 0.15.2. Proves the errdefer stream.close()
    // in init actually fired (otherwise we'd block waiting for a byte).
    var probe: [1]u8 = undefined;
    const probe_result = std.posix.read(fd_before, &probe);
    try std.testing.expectError(error.NotOpenForReading, probe_result);
}

test "Client.readyCheck self-heals when the server drops the connection" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start();
    defer fake.shutdown();

    const url = try fake.url(alloc);
    defer alloc.free(url);

    var client = try redis.testing.connectFromUrl(alloc, url);
    defer client.deinit();

    // Two consecutive readyChecks against a fake that drops after every
    // reply. Both must succeed: the first uses a live pre-warmed conn,
    // the second hits a now-dead idle conn → pool retry dials fresh and
    // succeeds. This is the Upstash idle-drop scenario the pool layer
    // recovers from transparently — without retry, /readyz would flip
    // queue:false on the second probe and stay there until restart.
    try client.readyCheck();
    try client.readyCheck();

    // At least two server-side accepts: the eager pre-warm dials plus
    // the redial during the second probe's retry path push this past
    // 2 in practice; assert the lower bound only so the test stays
    // robust against accept-ordering races between the fake's
    // single-threaded accept loop and the pool's parallel dials.
    try std.testing.expect(fake.accepts.load(.monotonic) >= 2);
}

