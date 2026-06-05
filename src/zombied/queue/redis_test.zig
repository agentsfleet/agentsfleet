const std = @import("std");
const common = @import("common");
const redis = @import("redis.zig");

/// Zig 0.16 moved sockets under `std.Io.net` (io-threaded Stream/Server).
const net = std.Io.net;
const test_port = @import("../http/test_port.zig");

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
    try std.testing.expectError(error.RedisTlsCaFileMustBeAbsolute, redis.testing.loadCaBundle(common.globalIo(), std.testing.allocator, "docker/redis/tls/ca.crt"));
}

test "integration: rediss ping via TEST_REDIS_TLS_URL" {
    const alloc = std.testing.allocator;
    const tls_url = common.env.testLiveValue("TEST_REDIS_TLS_URL") orelse return error.SkipZigTest;

    if (!std.mem.startsWith(u8, tls_url, "rediss://")) return error.SkipZigTest;

    var client = try redis.testing.connectFromUrl(common.globalIo(), alloc, tls_url);
    defer client.deinit();
    try client.ping();
}

test "roleEnvVarName maps redis roles deterministically" {
    try std.testing.expectEqualStrings("REDIS_URL", redis.roleEnvVarName(.default));
    try std.testing.expectEqualStrings("REDIS_URL_API", redis.roleEnvVarName(.api));
}

test "RedisRole carries no worker variant" {
    inline for (@typeInfo(redis.RedisRole).@"enum".fields) |field| {
        try std.testing.expect(!std.mem.eql(u8, field.name, "worker"));
    }
}

// ── Queue constants regression tests ─────────────────────────────────────

const queue_consts = @import("constants.zig");

test "queue constants: consumer prefix is stable" {
    try std.testing.expectEqualStrings("worker", queue_consts.consumer_prefix);
}

// pin test: literal is the contract — install/lease/report (redis_zombie.zig)
// all reference this one constant, so pinning the value pins the group used by
// every step. Renamed from "zombie_workers" (worker-substrate retirement).
test "queue constants: zombie consumer group reflects the lease path" {
    try std.testing.expectEqualStrings("zombie_lease", queue_consts.zombie_consumer_group);
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
    server: net.Server,
    port: u16,
    accept_thread: std.Thread,
    stop: std.atomic.Value(bool),
    accepts: std.atomic.Value(u32),

    fn start(self: *PingFake) !void {
        const io = common.globalIo();
        const lp = try test_port.listenLoopback(io);
        self.server = lp.server;
        self.port = lp.port;
        self.stop = std.atomic.Value(bool).init(false);
        self.accepts = std.atomic.Value(u32).init(0);
        self.accept_thread = try std.Thread.spawn(.{}, acceptLoop, .{self});
    }

    fn shutdown(self: *PingFake) void {
        const io = common.globalIo();
        self.stop.store(true, .release);
        // Wake the accept() by dialing ourselves; ignore failure (the
        // listener may already be torn down by the loop on error paths).
        var addr = net.IpAddress.parseIp4("127.0.0.1", self.port) catch return;
        if (addr.connect(io, .{ .mode = .stream })) |s| s.close(io) else |_| {}
        self.accept_thread.join();
        self.server.deinit(io);
    }

    fn acceptLoop(self: *PingFake) void {
        const io = common.globalIo();
        while (!self.stop.load(.acquire)) {
            const stream = self.server.accept(io) catch return;
            if (self.stop.load(.acquire)) {
                stream.close(io);
                return;
            }
            _ = self.accepts.fetchAdd(1, .monotonic);
            const t = std.Thread.spawn(.{}, handleConn, .{stream}) catch {
                stream.close(io);
                continue;
            };
            t.detach();
        }
    }

    fn handleConn(stream: net.Stream) void {
        const io = common.globalIo();
        defer stream.close(io);
        var read_buf: [256]u8 = undefined;
        var reader = stream.reader(io, &read_buf);
        var dest: [256]u8 = undefined;
        // Single read: `readSliceShort` loops until the buffer fills or EOF,
        // which deadlocks against a client that sends one short command then
        // waits for the reply. One `readVec` drains what's there.
        var dest_vec: [1][]u8 = .{&dest};
        const n = reader.interface.readVec(&dest_vec) catch 0;
        if (n == 0) return;
        var write_buf: [64]u8 = undefined;
        var writer = stream.writer(io, &write_buf);
        writer.interface.writeAll("+PONG\r\n") catch return;
        writer.interface.flush() catch return;
        // Drop the connection — emulates Upstash idle-drop / restart.
    }

    fn url(self: *PingFake, alloc: std.mem.Allocator) ![]u8 {
        return std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.port});
    }
};

test "PlainTransport.init enables SO_KEEPALIVE on its socket" {
    // Bind a loopback listener and dial it through PlainTransport so the
    // keepalive bit is exercised through the same path the real client uses.
    // SO_KEEPALIVE on the underlying fd is observable via getsockopt; the
    // per-OS keep-idle knobs are best-effort and not asserted.
    const io = common.globalIo();
    var lp = try test_port.listenLoopback(io);
    defer lp.server.deinit(io);

    var caddr = try net.IpAddress.parseIp4("127.0.0.1", lp.port);
    const stream = try caddr.connect(io, .{ .mode = .stream });
    var t = try redis_transport.PlainTransport.init(io, std.testing.allocator, stream);
    defer t.deinit(io, std.testing.allocator);

    var enabled: c_int = 0;
    var olen: std.posix.socklen_t = @sizeOf(c_int);
    if (std.c.getsockopt(
        t.stream.socket.handle,
        std.posix.SOL.SOCKET,
        std.posix.SO.KEEPALIVE,
        &enabled,
        &olen,
    ) != 0) return error.GetSockOptFailed;
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
    const io = common.globalIo();
    var lp = try test_port.listenLoopback(io);
    defer lp.server.deinit(io);

    var caddr = try net.IpAddress.parseIp4("127.0.0.1", lp.port);
    const stream = try caddr.connect(io, .{ .mode = .stream });
    const fd_before = stream.socket.handle;

    // FailingAllocator that errors on the 2nd allocation — read_buffer
    // succeeds (so init progresses past the first errdefer), write_buffer
    // fails (so init returns an error and our errdefer stream.close() must fire).
    var failing = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 1 });
    const result = redis_transport.PlainTransport.init(io, failing.allocator(), stream);
    try std.testing.expectError(error.OutOfMemory, result);

    // Confirm the fd is no longer open. read() on a closed fd returns EBADF,
    // which Zig 0.16's `std.posix.read` maps to `error.Unexpected` (it treats
    // a bad fd as use-after-free). Proves the errdefer stream.close() in init
    // actually fired (otherwise we'd block waiting for a byte).
    var probe: [1]u8 = undefined;
    const probe_result = std.posix.read(fd_before, &probe);
    try std.testing.expectError(error.Unexpected, probe_result);
}

test "Client.readyCheck self-heals when the server drops the connection" {
    const alloc = std.testing.allocator;

    var fake: PingFake = undefined;
    try fake.start();
    defer fake.shutdown();

    const url = try fake.url(alloc);
    defer alloc.free(url);

    var client = try redis.testing.connectFromUrl(common.globalIo(), alloc, url);
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
