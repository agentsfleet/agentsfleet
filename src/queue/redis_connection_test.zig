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
