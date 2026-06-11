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
const metrics = @import("../observability/metrics.zig");
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
/// "Park forever" relative to the wake bounds below — a consumer that only
/// returns at this deadline has missed its wake.
const LONG_POP_MS: u64 = 60_000;
/// Lets a spawned consumer reach its futex wait before the wake fires.
const PARK_SETTLE_NS: u64 = 50 * std.time.ns_per_ms;
/// Generous wake-latency ceiling: a real wake lands in microseconds; only the
/// LONG_POP_MS timeout path can exceed this.
const WAKE_BOUND_MS: i64 = 10_000;
/// hub.stop()'s bounded drain is 5s (STOP_DRAIN_MAX_MS) — anything past this
/// ceiling means the bound regressed toward a hang.
const STOP_BOUND_CEILING_MS: i64 = 9_000;
/// Nothing listens on port 1 — a deterministic connection-refused boot path.
const REFUSED_REDIS_URL = "redis://127.0.0.1:1";
/// 1.5× the hub's read timeout (HUB_READ_TIMEOUT_MS = 1s): long enough to
/// guarantee at least one full idle tick is observed.
const IDLE_TICK_OBSERVE_NS: u64 = 1_500 * std.time.ns_per_ms;
/// Subscription.create's two allocation sites precede push's payload copy.
const PUSH_COPY_FAIL_INDEX: usize = 2;

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

    const dropped_before = metrics.snapshot().sse_dropped_frames_total;
    var frame_buf: [16]u8 = undefined;
    var i: usize = 0;
    while (i < Subscription.QUEUE_CAPACITY + 3) : (i += 1) {
        const frame = try std.fmt.bufPrint(&frame_buf, "f{d}", .{i});
        sub.push(frame);
    }
    try testing.expectEqual(@as(u64, 3), sub.dropCount());
    // the local mirror and the operator counter must move together
    try testing.expectEqual(dropped_before + 3, metrics.snapshot().sse_dropped_frames_total);

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

test "subscription: queued frames drain before the closed signal" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    sub.push(PAYLOAD_ONE);
    sub.close();

    // queued frame delivered before the closed signal (shutdown drain)
    const first = sub.pop(SHORT_POP_MS);
    try testing.expect(first == .message);
    testing.allocator.free(first.message);
    try testing.expect(sub.pop(SHORT_POP_MS) == .closed);
}

/// One pop with a long deadline; records what came back and how long the
/// wait actually took, so wake-latency tests can assert the futex wake fired
/// instead of the deadline expiring.
const WakeProbe = struct {
    outcome: enum { none, message, timeout, closed } = .none,
    elapsed_ms: i64 = 0,
};

fn popOnceIntoProbe(sub: *Subscription, probe: *WakeProbe) void {
    const start_ms = common.clock.nowMillis();
    switch (sub.pop(LONG_POP_MS)) {
        .message => |payload| {
            std.testing.allocator.free(payload);
            probe.outcome = .message;
        },
        .timeout => probe.outcome = .timeout,
        .closed => probe.outcome = .closed,
    }
    probe.elapsed_ms = common.clock.nowMillis() - start_ms;
}

test "subscription: push wakes a consumer already parked in pop" {
    // The core liveness property of the epoch-futex protocol: a parked
    // consumer is woken by the producer's bump-then-wake, not by its deadline.
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    var probe: WakeProbe = .{};
    const consumer = try std.Thread.spawn(.{}, popOnceIntoProbe, .{ sub, &probe });
    common.sleepNanos(PARK_SETTLE_NS);
    sub.push(PAYLOAD_ONE);
    consumer.join();

    try testing.expect(probe.outcome == .message);
    try testing.expect(probe.elapsed_ms < WAKE_BOUND_MS);
}

test "subscription: close wakes a consumer already parked in pop" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    var probe: WakeProbe = .{};
    const consumer = try std.Thread.spawn(.{}, popOnceIntoProbe, .{ sub, &probe });
    common.sleepNanos(PARK_SETTLE_NS);
    sub.close();
    consumer.join();

    try testing.expect(probe.outcome == .closed);
    try testing.expect(probe.elapsed_ms < WAKE_BOUND_MS);
}

test "subscription: push after close is freed, never delivered" {
    // Guards the hub-dispatch-races-stop window: a frame arriving after close
    // must not resurrect the queue, and its copy must be freed (the leak
    // detector is half the assertion).
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    sub.close();
    sub.push(PAYLOAD_ONE);
    try testing.expect(sub.pop(SHORT_POP_MS) == .closed);
}

test "subscription: a failed payload copy counts a drop and delivers nothing" {
    var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = PUSH_COPY_FAIL_INDEX });
    const sub = try Subscription.create(failing.allocator(), common.globalIo(), CHANNEL_A);
    defer sub.destroy();

    const dropped_before = metrics.snapshot().sse_dropped_frames_total;
    sub.push(PAYLOAD_ONE); // alloc.dupe fails → noteDrop, no enqueue
    try testing.expectEqual(@as(u64, 1), sub.dropCount());
    try testing.expectEqual(dropped_before + 1, metrics.snapshot().sse_dropped_frames_total);
    try testing.expect(sub.pop(SHORT_POP_MS) == .timeout);
}

test "subscription: create unwinds cleanly under allocation failure" {
    try std.testing.checkAllAllocationFailures(testing.allocator, createDestroy, .{});
}

fn createDestroy(alloc: std.mem.Allocator) !void {
    const sub = try Subscription.create(alloc, common.globalIo(), CHANNEL_A);
    sub.destroy();
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
        switch (sub.pop(LONG_POP_MS)) {
            .message => |payload| std.testing.allocator.free(payload),
            .timeout => continue,
            .closed => break,
        }
    }
    hub.unsubscribe(sub);
}

test "hub: stop is idempotent" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    hub.stop();
    hub.stop(); // second call must return immediately, no double-teardown
}

test "hub: stop returns bounded even when a subscriber never detaches" {
    // The shutdown-hang guarantee itself: a stream thread that never reaches
    // its unsubscribe must cost stop() at most its bounded drain (5s), never
    // a wedge. Wall cost of this test is that bound — deliberate.
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();

    const sub = try hub.subscribe(CHANNEL_A);
    const start_ms = common.clock.nowMillis();
    hub.stop();
    const elapsed_ms = common.clock.nowMillis() - start_ms;
    try testing.expect(elapsed_ms < STOP_BOUND_CEILING_MS);
    // the undetached channel is still mapped — stop warned, not wedged
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
    hub.unsubscribe(sub); // detach late so deinit sees an empty map
}

test "hub: subscribe unwinds cleanly under allocation failure" {
    // Walks every allocation site in subscribe/attach — including the
    // fresh-slot append-failure rollback — and proves no leak on any of them.
    try std.testing.checkAllAllocationFailures(testing.allocator, subscribeUnsubscribe, .{});
    try std.testing.checkAllAllocationFailures(testing.allocator, subscribeTwiceUnsubscribe, .{});
}

fn subscribeUnsubscribe(alloc: std.mem.Allocator) !void {
    var hub = subscription_hub.init(alloc, common.globalIo());
    defer hub.deinit();
    const sub = try hub.subscribe(CHANNEL_A);
    hub.unsubscribe(sub);
}

fn subscribeTwiceUnsubscribe(alloc: std.mem.Allocator) !void {
    var hub = subscription_hub.init(alloc, common.globalIo());
    defer hub.deinit();
    const first = try hub.subscribe(CHANNEL_A);
    errdefer hub.unsubscribe(first);
    const second = try hub.subscribe(CHANNEL_A); // found_existing append path
    hub.unsubscribe(second);
    hub.unsubscribe(first);
}

test "hub: start propagates a refused connection and tears down cleanly" {
    // Boot-path mirror of the queue client connect: nothing listens on the
    // configured port, start must error, and stop/deinit on the failed hub
    // must neither crash nor leak.
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, REFUSED_REDIS_URL);
    defer redis_config.deinitConfig(testing.allocator, cfg);

    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    if (hub.start(cfg)) |_| return error.TestUnexpectedResult else |_| {}
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

    // a quiet read-timeout tick is NOT a dead socket: observing a full idle
    // window must not move the reconnect counter (a regression here is a
    // redial storm against a healthy Redis)
    const reconnects_before = metrics.snapshot().sse_hub_reconnects_total;
    common.sleepNanos(IDLE_TICK_OBSERVE_NS);
    try testing.expectEqual(reconnects_before, metrics.snapshot().sse_hub_reconnects_total);

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
    const reconnects_before = metrics.snapshot().sse_hub_reconnects_total;
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
    // the redial is what the operator counter counts
    try testing.expect(metrics.snapshot().sse_hub_reconnects_total >= reconnects_before + 1);
}

test "integration: a stalled viewer drops oldest while its channel sibling receives everything" {
    // Per-viewer ring isolation under real fan-out: one stalled consumer must
    // cost only its own oldest frames — its sibling on the SAME channel sees
    // the full ordered sequence and the hub never blocks.
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg);

    const active = try hub.subscribe(CHANNEL_A);
    defer hub.unsubscribe(active);
    const stalled = try hub.subscribe(CHANNEL_A);
    defer hub.unsubscribe(stalled);
    // wait for the wire SUBSCRIBE to settle — frames published before the
    // server registers the subscription are lost, not queued
    try expectNumsub(&pub_client, CHANNEL_A, 1);

    const total = Subscription.QUEUE_CAPACITY + 3;
    var frame_buf: [16]u8 = undefined;
    var i: usize = 0;
    while (i < total) : (i += 1) {
        const frame = try std.fmt.bufPrint(&frame_buf, "f{d}", .{i});
        try pub_client.publish(CHANNEL_A, frame);
    }

    // the active sibling receives every frame, in order
    var expect_buf: [16]u8 = undefined;
    i = 0;
    while (i < total) : (i += 1) {
        const got = active.pop(DELIVERY_WAIT_MS);
        try testing.expect(got == .message);
        defer testing.allocator.free(got.message);
        const expected = try std.fmt.bufPrint(&expect_buf, "f{d}", .{i});
        try testing.expectEqualStrings(expected, got.message);
    }

    // the stalled viewer dropped exactly the overflow, oldest-first
    var waited_ms: u64 = 0;
    while (stalled.dropCount() < 3 and waited_ms < DELIVERY_WAIT_MS) : (waited_ms += 100) {
        common.sleepNanos(POLL_SLEEP_NS);
    }
    try testing.expectEqual(@as(u64, 3), stalled.dropCount());
    const head = stalled.pop(SHORT_POP_MS);
    try testing.expect(head == .message);
    defer testing.allocator.free(head.message);
    try testing.expectEqualStrings("f3", head.message);
}
