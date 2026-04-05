//! M22_001 §2 — integration tests for Redis pub/sub subscriber.
//!
//! Requires TEST_REDIS_TLS_URL. Skipped when no Redis is available.
//!
//! NOTE: Tests that depend on SO_RCVTIMEO firing within a short window are
//! excluded. Zig 0.15.2 Io.Reader converts socket-level WouldBlock into
//! ReadFailed, and the TLS record layer may block past the socket timeout
//! on Linux. Only tests with actual published messages are safe — readMessage
//! returns immediately when data is available.

const std = @import("std");
const redis_pubsub = @import("redis_pubsub.zig");
const redis_client = @import("redis_client.zig");

fn connectTestSubscriber(alloc: std.mem.Allocator) !?redis_pubsub.Subscriber {
    const tls_url = std.process.getEnvVarOwned(alloc, "TEST_REDIS_TLS_URL") catch return null;
    defer alloc.free(tls_url);
    return redis_pubsub.Subscriber.connectFromUrl(alloc, tls_url) catch return null;
}

fn connectTestPublisher(alloc: std.mem.Allocator) !?redis_client.Client {
    const tls_url = std.process.getEnvVarOwned(alloc, "TEST_REDIS_TLS_URL") catch return null;
    defer alloc.free(tls_url);
    return redis_client.Client.connectFromUrl(alloc, tls_url) catch return null;
}

// ---------------------------------------------------------------------------
// T10: Constants
// ---------------------------------------------------------------------------

test "T10: PUBSUB_READ_TIMEOUT_MS is 25000 and less than 30s proxy threshold" {
    try std.testing.expectEqual(@as(u32, 25_000), redis_pubsub.PUBSUB_READ_TIMEOUT_MS);
    try std.testing.expect(redis_pubsub.PUBSUB_READ_TIMEOUT_MS < 30_000);
}

// ---------------------------------------------------------------------------
// Integration: publish then readMessage delivers correctly
// ---------------------------------------------------------------------------

test "integration: published message is received by subscriber" {
    const alloc = std.testing.allocator;
    var sub = (try connectTestSubscriber(alloc)) orelse return error.SkipZigTest;
    defer sub.deinit();
    var pub_client = (try connectTestPublisher(alloc)) orelse return error.SkipZigTest;
    defer pub_client.deinit();

    const channel = try std.fmt.allocPrint(alloc, "test:m22:pubsub:{d}", .{std.time.milliTimestamp()});
    defer alloc.free(channel);

    try sub.subscribe(channel);
    std.Thread.sleep(200 * std.time.ns_per_ms);

    const payload = "{\"gate_name\":\"lint\",\"outcome\":\"PASS\",\"created_at\":1700000000000}";
    try pub_client.publish(channel, payload);

    const msg_opt = try sub.readMessage();
    try std.testing.expect(msg_opt != null);

    var msg = msg_opt.?;
    defer msg.deinit();
    try std.testing.expectEqualStrings(payload, msg.data);
    try std.testing.expectEqualStrings(channel, msg.channel);
}

// ---------------------------------------------------------------------------
// Integration: multiple messages delivered in order
// ---------------------------------------------------------------------------

test "integration: multiple published messages arrive in order" {
    const alloc = std.testing.allocator;
    var sub = (try connectTestSubscriber(alloc)) orelse return error.SkipZigTest;
    defer sub.deinit();
    var pub_client = (try connectTestPublisher(alloc)) orelse return error.SkipZigTest;
    defer pub_client.deinit();

    const channel = try std.fmt.allocPrint(alloc, "test:m22:order:{d}", .{std.time.milliTimestamp()});
    defer alloc.free(channel);

    try sub.subscribe(channel);
    std.Thread.sleep(200 * std.time.ns_per_ms);

    try pub_client.publish(channel, "msg-1");
    try pub_client.publish(channel, "msg-2");
    try pub_client.publish(channel, "msg-3");

    var msg1 = (try sub.readMessage()).?;
    defer msg1.deinit();
    try std.testing.expectEqualStrings("msg-1", msg1.data);

    var msg2 = (try sub.readMessage()).?;
    defer msg2.deinit();
    try std.testing.expectEqualStrings("msg-2", msg2.data);

    var msg3 = (try sub.readMessage()).?;
    defer msg3.deinit();
    try std.testing.expectEqualStrings("msg-3", msg3.data);
}

// ---------------------------------------------------------------------------
// T11: Memory — PubSubMessage deinit frees owned data
// ---------------------------------------------------------------------------

test "integration: PubSubMessage deinit has no leaks over 20 messages" {
    const alloc = std.testing.allocator;
    var sub = (try connectTestSubscriber(alloc)) orelse return error.SkipZigTest;
    defer sub.deinit();
    var pub_client = (try connectTestPublisher(alloc)) orelse return error.SkipZigTest;
    defer pub_client.deinit();

    const channel = try std.fmt.allocPrint(alloc, "test:m22:memleak:{d}", .{std.time.milliTimestamp()});
    defer alloc.free(channel);

    try sub.subscribe(channel);
    std.Thread.sleep(200 * std.time.ns_per_ms);

    for (0..20) |i| {
        const payload = try std.fmt.allocPrint(alloc, "leak-check-{d}", .{i});
        defer alloc.free(payload);
        try pub_client.publish(channel, payload);

        var msg = (try sub.readMessage()).?;
        msg.deinit();
    }
}
