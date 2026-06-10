//! SubscriptionHub + Subscription tests.
//!
//! Pure tests cover the queue mechanics (FIFO, drop-oldest, timed pop,
//! close/drain) and the refcount map on a cold hub — wire sends are skipped
//! when no connection exists, which is exactly the reconnect-gap code path.
//!
//! Live-Redis tests (gated on the integration env, like the SSE fixtures)
//! prove the wire facts: one server-side subscriber per channel regardless
//! of local viewer count, refcounted UNSUBSCRIBE, fan-out delivery, and
//! recovery after the shared connection is killed out from under the hub.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const queue_redis = @import("../queue/redis.zig");
const redis_config = @import("../queue/redis_config.zig");
const subscription_hub = @import("subscription_hub.zig");
const Subscription = subscription_hub.Subscription;

const TEST_REDIS_URL_ENV = "TEST_REDIS_TLS_URL";
const CHANNEL_A = "hubtest:alpha:activity";
const CHANNEL_B = "hubtest:beta:activity";
const PAYLOAD_ONE = "{\"kind\":\"chunk\",\"n\":1}";
const SHORT_POP_MS: u64 = 50;
const DELIVERY_WAIT_MS: u64 = 5_000;
const RECOVERY_WAIT_MS: u64 = 15_000;
const POLL_SLEEP_NS: u64 = 100 * std.time.ns_per_ms;

// ── Subscription mechanics (pure) ───────────────────────────────────────────

test "subscription: frames pop in publish order and ownership transfers" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    sub.push("first");
    sub.push("second");

    const a = sub.pop(SHORT_POP_MS);
    try testing.expect(a == .message);
    defer testing.allocator.free(a.message);
    try testing.expectEqualStrings("first", a.message);

    const b = sub.pop(SHORT_POP_MS);
    try testing.expect(b == .message);
    defer testing.allocator.free(b.message);
    try testing.expectEqualStrings("second", b.message);
}

test "subscription: full ring drops oldest, counts it, keeps newest" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    var frame_buf: [16]u8 = undefined;
    var i: usize = 0;
    while (i < Subscription.QUEUE_CAPACITY + 3) : (i += 1) {
        const frame = try std.fmt.bufPrint(&frame_buf, "f{d}", .{i});
        sub.push(frame);
    }
    try testing.expectEqual(@as(u64, 3), sub.dropCount());

    // oldest survivor is frame 3 — 0..2 were evicted
    const head = sub.pop(SHORT_POP_MS);
    try testing.expect(head == .message);
    defer testing.allocator.free(head.message);
    try testing.expectEqualStrings("f3", head.message);
}

test "subscription: pop times out on a quiet queue" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();
    try testing.expect(sub.pop(SHORT_POP_MS) == .timeout);
}

test "subscription: close wakes a parked consumer; queued frames drain first" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    sub.push(PAYLOAD_ONE);
    sub.close();

    // queued frame delivered before the closed signal (shutdown drain)
    const first = sub.pop(SHORT_POP_MS);
    try testing.expect(first == .message);
    testing.allocator.free(first.message);
    try testing.expect(sub.pop(SHORT_POP_MS) == .closed);

    // and a parked consumer wakes promptly on close (no full timeout wait)
    const consumer = try std.Thread.spawn(.{}, popUntilClosed, .{sub});
    consumer.join();
}

fn popUntilClosed(sub: *Subscription) void {
    while (true) {
        switch (sub.pop(60_000)) {
            .message => |payload| std.testing.allocator.free(payload),
            .timeout => continue,
            .closed => return,
        }
    }
}

// ── Hub refcount map (cold hub — no connection, the reconnect-gap path) ─────

test "hub: refcounted channel map — wire-silent middles, last-out removes" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    const s1 = try hub.subscribe(CHANNEL_A);
    const s2 = try hub.subscribe(CHANNEL_A);
    const s3 = try hub.subscribe(CHANNEL_A);
    const sb = try hub.subscribe(CHANNEL_B);
    try testing.expectEqual(@as(usize, 2), hub.channelCount());

    hub.unsubscribe(s1);
    hub.unsubscribe(s2);
    try testing.expectEqual(@as(usize, 2), hub.channelCount());
    hub.unsubscribe(s3);
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
    hub.unsubscribe(sb);
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}

test "hub: stop closes live subscriptions and rejects new subscribes" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();

    const sub = try hub.subscribe(CHANNEL_A);
    // consumer mirrors streamThreadMain: drain to .closed, then unsubscribe —
    // which is also what stop()'s bounded drain waits on
    const consumer = try std.Thread.spawn(.{}, drainThenUnsubscribe, .{ &hub, sub });
    hub.stop();
    consumer.join();
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
    try testing.expectError(error.HubStopped, hub.subscribe(CHANNEL_A));
}

fn drainThenUnsubscribe(hub: *subscription_hub, sub: *Subscription) void {
    while (true) {
        switch (sub.pop(60_000)) {
            .message => |payload| std.testing.allocator.free(payload),
            .timeout => continue,
            .closed => break,
        }
    }
    hub.unsubscribe(sub);
}

// ── Live-Redis wire tests (integration env, same gating as the SSE suites) ──

fn requireRedisUrlOrSkip() ![]const u8 {
    return common.env.testLiveValue(TEST_REDIS_URL_ENV) orelse return error.SkipZigTest;
}

/// Server-side subscriber count for `channel` (`PUBSUB NUMSUB`) — the wire
/// truth the refcount is supposed to compress to 0 or 1.
fn numsub(client: *queue_redis.Client, channel: []const u8) !i64 {
    var reply = try client.command(&.{ "PUBSUB", "NUMSUB", channel });
    defer reply.deinit(testing.allocator);
    if (reply != .array) return error.TestUnexpectedResult;
    const arr = reply.array orelse return error.TestUnexpectedResult;
    if (arr.len < 2 or arr[1] != .integer) return error.TestUnexpectedResult;
    return arr[1].integer;
}

fn expectNumsub(client: *queue_redis.Client, channel: []const u8, want: i64) !void {
    // PUBSUB state on the server settles asynchronously after a send
    var waited_ms: u64 = 0;
    while (waited_ms < DELIVERY_WAIT_MS) : (waited_ms += 100) {
        if (try numsub(client, channel) == want) return;
        common.sleepNanos(POLL_SLEEP_NS);
    }
    try testing.expectEqual(want, try numsub(client, channel));
}

test "integration: hub holds one wire subscriber per channel for N viewers; fan-out reaches all" {
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg);

    const s1 = try hub.subscribe(CHANNEL_A);
    const s2 = try hub.subscribe(CHANNEL_A);
    const s3 = try hub.subscribe(CHANNEL_A);
    const sb = try hub.subscribe(CHANNEL_B);
    var live = [_]?*Subscription{ s1, s2, s3, sb };
    defer for (&live) |*maybe| {
        if (maybe.*) |sub| hub.unsubscribe(sub);
    };

    // three local viewers, ONE wire subscriber (the dedup this design buys)
    try expectNumsub(&pub_client, CHANNEL_A, 1);
    try expectNumsub(&pub_client, CHANNEL_B, 1);

    // fan-out: one publish reaches every viewer of A and no viewer of B
    try pub_client.publish(CHANNEL_A, PAYLOAD_ONE);
    for ([_]*Subscription{ s1, s2, s3 }) |sub| {
        const got = sub.pop(DELIVERY_WAIT_MS);
        try testing.expect(got == .message);
        defer testing.allocator.free(got.message);
        try testing.expectEqualStrings(PAYLOAD_ONE, got.message);
    }
    try testing.expect(sb.pop(SHORT_POP_MS) == .timeout);

    // refcount edges on the wire: middles silent, last-out unsubscribes
    hub.unsubscribe(s1);
    live[0] = null;
    hub.unsubscribe(s2);
    live[1] = null;
    try expectNumsub(&pub_client, CHANNEL_A, 1);
    hub.unsubscribe(s3);
    live[2] = null;
    try expectNumsub(&pub_client, CHANNEL_A, 0);
}

test "integration: hub reconnects after its connection is killed and delivery resumes" {
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg);

    const sub = try hub.subscribe(CHANNEL_A);
    defer hub.unsubscribe(sub);
    try expectNumsub(&pub_client, CHANNEL_A, 1);

    // sever the shared connection out from under the hub
    var kill_reply = try pub_client.command(&.{ "CLIENT", "KILL", "TYPE", "pubsub" });
    kill_reply.deinit(testing.allocator);

    // the viewer notices nothing; publish-poll until the redialed connection
    // and its re-SUBSCRIBE sweep deliver again
    var waited_ms: u64 = 0;
    var recovered = false;
    while (waited_ms < RECOVERY_WAIT_MS) : (waited_ms += 200) {
        try pub_client.publish(CHANNEL_A, PAYLOAD_ONE);
        switch (sub.pop(200)) {
            .message => |payload| {
                testing.allocator.free(payload);
                recovered = true;
                break;
            },
            .timeout => continue,
            .closed => return error.TestUnexpectedResult,
        }
    }
    try testing.expect(recovered);
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
}
