//! Integration tests for the unified `Subscriber` (slice 4) — exercises
//! `connectFromUrl` against a real Redis broker so the `SO_RCVTIMEO`
//! install-after-subscribe-ack path and the `nextMessage → null` swallow
//! on read timeout are covered end-to-end. Unit-shape coverage of the
//! subscribe-ack parser lives inside `redis_subscriber.zig`; the broker
//! consumers in `src/zombie/event_loop_harness_*_test.zig` exercise the
//! blocking-mode `.{ .read_timeout_ms = N }` path under a real workload.
//!
//! Skip-by-default unless `TEST_REDIS_TLS_URL=rediss://...` is exported.
//! Pattern matches `redis_test.zig` "integration: rediss ping".

const std = @import("std");
const Subscriber = @import("redis_subscriber.zig");

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

// In-process fake that:
//   1. accepts one TCP connection
//   2. drains the SUBSCRIBE command bytes
//   3. writes a `*3\r\n$9\r\nsubscribe\r\n$<n>\r\n<chan>\r\n:1\r\n` ack
//   4. sleeps `publish_delay_ms` (simulates a publisher pushing after a delay)
//   5. writes a `*3\r\n$7\r\nmessage\r\n$<n>\r\n<chan>\r\n$<n>\r\n<payload>\r\n` push
//   6. holds the socket open until shutdown
//
// Used to prove `nextMessage()` with `read_timeout_ms = null` blocks past
// the ack and returns the delivered Message — the production blocking path.
const SubscribeAckThenMessage = struct {
    server: std.net.Server,
    addr: std.net.Address,
    thread: std.Thread,
    stop: std.atomic.Value(bool),
    channel: []const u8,
    payload: []const u8,
    publish_delay_ms: u64,

    fn start(self: *SubscribeAckThenMessage, channel: []const u8, payload: []const u8, publish_delay_ms: u64) !void {
        const loopback = try std.net.Address.parseIp4("127.0.0.1", 0);
        self.server = try loopback.listen(.{ .reuse_address = true });
        self.addr = self.server.listen_address;
        self.stop = std.atomic.Value(bool).init(false);
        self.channel = channel;
        self.payload = payload;
        self.publish_delay_ms = publish_delay_ms;
        self.thread = try std.Thread.spawn(.{}, loop, .{self});
    }

    fn shutdown(self: *SubscribeAckThenMessage) void {
        self.stop.store(true, .release);
        if (std.net.tcpConnectToAddress(self.addr)) |s| s.close() else |_| {}
        self.thread.join();
        self.server.deinit();
    }

    fn loop(self: *SubscribeAckThenMessage) void {
        const conn = self.server.accept() catch return;
        if (self.stop.load(.acquire)) {
            conn.stream.close();
            return;
        }
        var drain: [256]u8 = undefined;
        _ = conn.stream.read(&drain) catch {};

        var ack_buf: [256]u8 = undefined;
        const ack = std.fmt.bufPrint(
            &ack_buf,
            "*3\r\n$9\r\nsubscribe\r\n${d}\r\n{s}\r\n:1\r\n",
            .{ self.channel.len, self.channel },
        ) catch {
            conn.stream.close();
            return;
        };
        conn.stream.writeAll(ack) catch {
            conn.stream.close();
            return;
        };

        std.Thread.sleep(self.publish_delay_ms * std.time.ns_per_ms);

        var msg_buf: [512]u8 = undefined;
        const msg = std.fmt.bufPrint(
            &msg_buf,
            "*3\r\n$7\r\nmessage\r\n${d}\r\n{s}\r\n${d}\r\n{s}\r\n",
            .{ self.channel.len, self.channel, self.payload.len, self.payload },
        ) catch {
            conn.stream.close();
            return;
        };
        conn.stream.writeAll(msg) catch {};

        while (!self.stop.load(.acquire)) std.Thread.sleep(10 * std.time.ns_per_ms);
        conn.stream.close();
    }

    fn url(self: *SubscribeAckThenMessage, alloc: std.mem.Allocator) ![]u8 {
        return try std.fmt.allocPrint(alloc, "redis://127.0.0.1:{d}", .{self.addr.in.getPort()});
    }
};

test "Subscriber.nextMessage with read_timeout_ms=null blocks until message arrives" {
    // Spec: production path uses `read_timeout_ms = null` (block forever).
    // The fake delays its `message` push by 100ms after the SUBSCRIBE ack;
    // `nextMessage()` must NOT return null in that window (no SO_RCVTIMEO
    // armed → read blocks), and must return the published payload when the
    // server eventually pushes it.
    const alloc = std.testing.allocator;
    const channel = "test:sub:blocking";
    const payload = "hello-blocking-world";
    const publish_delay_ms: u64 = 100;

    var fake: SubscribeAckThenMessage = undefined;
    try fake.start(channel, payload, publish_delay_ms);
    defer fake.shutdown();

    const url = try fake.url(alloc);
    defer alloc.free(url);

    var sub = try Subscriber.connectFromUrl(alloc, url, .{ .read_timeout_ms = null });
    defer sub.deinit();
    try sub.subscribe(channel);

    const start = std.time.nanoTimestamp();
    const maybe_msg = try sub.nextMessage();
    const elapsed_ns = std.time.nanoTimestamp() - start;

    // Must have a message — null would mean stream closed or timeout fired.
    try std.testing.expect(maybe_msg != null);
    var got = maybe_msg.?;
    defer got.deinit(alloc);
    try std.testing.expectEqualStrings(channel, got.channel);
    try std.testing.expectEqualStrings(payload, got.payload);

    // Elapsed proves we actually blocked through the publisher's delay
    // (lower bound generous for CI jitter; upper bound catches retry / spin
    // bugs in the read loop).
    try std.testing.expect(elapsed_ns >= 50 * std.time.ns_per_ms);
    try std.testing.expect(elapsed_ns < 5 * std.time.ns_per_s);
}

test "integration: subscriber with 100ms read_timeout returns null on a quiet channel" {
    const alloc = std.testing.allocator;
    const tls_url = try tlsUrlOrSkip(alloc);
    defer alloc.free(tls_url);

    var sub = try Subscriber.connectFromUrl(alloc, tls_url, .{ .read_timeout_ms = 100 });
    defer sub.deinit();

    // Channel name unique enough that no other test or live worker would
    // PUBLISH against it during this run. The subscribe-ack handshake must
    // complete (it has no SO_RCVTIMEO in flight yet — set post-ack only)
    // before we sit on nextMessage.
    try sub.subscribe("test:subscriber:quiet-channel");

    const start = std.time.nanoTimestamp();
    const msg = try sub.nextMessage();
    const elapsed_ns = std.time.nanoTimestamp() - start;

    try std.testing.expect(msg == null);
    // Timer floor: kernel SO_RCVTIMEO granularity + RESP parser overhead
    // typically lands ≥50ms when the budget is 100ms. Generous upper bound
    // (5s) prevents this from flaking on a loaded CI host; the budget tells
    // us "fired roughly on time" not "fired exactly at 100ms".
    try std.testing.expect(elapsed_ns >= 50 * std.time.ns_per_ms);
    try std.testing.expect(elapsed_ns < 5 * std.time.ns_per_s);
}
