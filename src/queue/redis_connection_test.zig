//! Slice-1 tests for `Connection.init(role)` + `Connection.command`. Uses a
//! minimal in-process RESP fake (one accept, one "+PONG\r\n" reply, park
//! until shutdown) so tests don't depend on a live Redis. Sister tests at
//! the Pool level live in `redis_pool_test.zig`.

const std = @import("std");
const Connection = @import("redis_connection.zig");
const redis_config = @import("redis_config.zig");

const PongOnce = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),

    fn start(self: *PongOnce) !void {
        const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *PongOnce) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *PongOnce) void {
        while (!self.stop.load(.acquire)) {
            const conn = self.server.accept() catch return;
            if (self.stop.load(.acquire)) {
                conn.stream.close();
                return;
            }
            var buf: [256]u8 = undefined;
            const n = conn.stream.read(&buf) catch 0;
            if (n > 0) conn.stream.writeAll("+PONG\r\n") catch {};
            while (!self.stop.load(.acquire)) std.Thread.sleep(5 * std.time.ns_per_ms);
            conn.stream.close();
        }
    }

    fn config(self: *PongOnce, alloc: std.mem.Allocator) !redis_config.Config {
        const host = try alloc.dupe(u8, "127.0.0.1");
        return .{ .host = host, .port = self.addr.in.getPort(), .use_tls = false };
    }
};

test "init(.blocking_consumer) yields a live fd; deinit zeros it" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .blocking_consumer);
    try std.testing.expectEqual(Connection.Role.blocking_consumer, conn.role);
    try std.testing.expect(conn.fd != Connection.INVALID_FD);
    conn.deinit();
    try std.testing.expectEqual(Connection.INVALID_FD, conn.fd);
}

test "init(.subscriber) sets role and dials cleanly" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .subscriber);
    defer conn.deinit();
    try std.testing.expectEqual(Connection.Role.subscriber, conn.role);
}

test "command round-trips a single RESP reply" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .pooled);
    defer conn.deinit();

    var resp = try conn.command(&.{"PING"});
    defer resp.deinit(std.testing.allocator);
    try std.testing.expectEqualStrings("PONG", resp.simple);
}

test "post-deinit Connection has INVALID_FD and .closed state — IO call paths assert in debug" {
    // Spec invariant 13: after `deinit`, every subsequent IO path must
    // assert. The IO entry points (`command` → `commandAllowError`) start
    // with `std.debug.assert(self.fd != INVALID_FD)` and
    // `std.debug.assert(self.state == .active)`. We can't catch panics in
    // a zig 0.15 unit test (no `expectPanic`), but we CAN verify both
    // guarded preconditions hold post-deinit — calling `commandAllowError`
    // here would fire either assert deterministically in debug builds.
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .pooled);
    try std.testing.expect(conn.fd != Connection.INVALID_FD);
    try std.testing.expect(conn.state == .active);

    conn.deinit();

    try std.testing.expectEqual(Connection.INVALID_FD, conn.fd);
    try std.testing.expect(conn.state == .closed);
}

test "post-deinit Connection has .closed state — double-deinit asserts in debug" {
    // Spec invariant 14: `.closed` is reachable exactly once per Connection
    // lifetime. `deinit`'s state switch lists `.closed => std.debug.assert(false)` —
    // a second call would fire that assert in debug. Same constraint as the
    // IO test above: we can't catch the panic from inside the same process,
    // so we verify the post-first-deinit state is `.closed`, which is the
    // precondition the second-deinit assert checks. `transitionTo` itself
    // guards `.closed` as a terminal state (Invariant 14): no transition
    // out of `.closed` is legal, so even bypassing deinit's switch via the
    // direct mutation path lands on the same assertion gate.
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .pooled);
    try std.testing.expect(conn.state == .active);

    conn.deinit();
    try std.testing.expect(conn.state == .closed);
    // A subsequent `conn.deinit()` would hit `.closed => std.debug.assert(false)`
    // at redis_connection.zig and panic the test runner; the single-transition
    // invariant is enforced by the precondition observable here.
}

test "commandAllowError: OOM during RESP read propagates and poisons the connection" {
    var srv: PongOnce = undefined;
    try srv.start();
    defer srv.shutdown();

    const cfg = try srv.config(std.testing.allocator);
    defer redis_config.deinitConfig(std.testing.allocator, cfg);

    var conn = try Connection.init(std.testing.allocator, &cfg, .pooled);
    // Restore the real allocator before deinit so transport cleanup uses
    // the same allocator that init dialed with.
    defer {
        conn.alloc = std.testing.allocator;
        conn.deinit();
    }

    // FailingAllocator with fail_index=0 returns OutOfMemory on every
    // allocation. The argv write path doesn't allocate, so the WRITE
    // round-trip completes; readRespValue's first allocation inside the
    // RESP parser fails.
    var fa = std.testing.FailingAllocator.init(std.testing.allocator, .{ .fail_index = 0 });
    conn.alloc = fa.allocator();

    const result = conn.commandAllowError(&.{"PING"});
    try std.testing.expectError(error.OutOfMemory, result);

    // Load-bearing assertion: OOM during RESP read must poison the
    // connection because bulk-string (`$N\r\n…`) and array (`*N\r\n…`)
    // replies consume the length/count header before the body allocation
    // fires — partial bytes remain in the transport buffer and the next
    // RESP read would start mid-message. The OOM still surfaces verbatim
    // (no `ReadFailed` re-tag) AND Pool.release(conn, true) sees the
    // poisoned state and closes the conn.
    try std.testing.expect(conn.state == .poisoned);
}
