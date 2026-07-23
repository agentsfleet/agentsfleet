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
const call_deadline = @import("call_deadline");
/// One scheduler per test, declared before the hub so its deinit unwinds LAST —
/// the hub's wire registrations must be finished before the scheduler storage
/// they live in is freed.
const TestScheduler = struct {
    backend: call_deadline.MonotonicBackend = .{},
    sched: ?call_deadline.ProcessScheduler = null,
    /// Concurrency-capable Io for the hub under test. `common.globalIo()` is
    /// statically single-threaded, so the raced dial would fail on it.
    hub_io: ?std.Io.Threaded = null,

    fn io(self: *TestScheduler) std.Io {
        if (self.hub_io == null) self.hub_io = std.Io.Threaded.init(testing.allocator, .{});
        return self.hub_io.?.io();
    }

    fn start(self: *TestScheduler) !*call_deadline.ProcessScheduler {
        self.sched = call_deadline.ProcessScheduler.init(testing.allocator, &self.backend);
        try self.sched.?.start();
        return &self.sched.?;
    }

    fn deinit(self: *TestScheduler) void {
        if (self.sched) |*s| s.deinit();
        if (self.hub_io) |*t| t.deinit();
    }
};

const Subscription = subscription_hub.Subscription;

const TEST_REDIS_URL_ENV = "TEST_REDIS_TLS_URL";
const CHANNEL_A = "hubtest:alpha:activity";
const CHANNEL_B = "hubtest:beta:activity";
/// Owned exclusively by the stalled-viewer test: a channel no other test
/// subscribes, so its NUMSUB settle can only be satisfied by THIS hub's wire
/// SUBSCRIBE — a prior test's dying connection on a shared channel could
/// otherwise hold the count at 1 while early publishes go to the dead socket.
const CHANNEL_ISOLATION = "hubtest:gamma:activity";
const PAYLOAD_ONE = "{\"kind\":\"chunk\",\"n\":1}";
const PAYLOAD_TWO = "{\"kind\":\"chunk\",\"n\":2}";
/// A shared consumer's label — a workspace id in production; it names the
/// consumer in logs and is never a channel map key.
const WS_LABEL = "hubtest-workspace";
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

// Process-level RSS soak: the coarse (Bun-style) leak layer for hub churn on a
// PRODUCTION general-purpose allocator (smp) — the map + Subscription
// alloc/free retention testing.allocator's leak detector can't see (that exact
// in-process oracle is the checkAllAllocationFailures suite above). A cold hub
// (no connection) is the reconnect-gap path: wireSend early-returns when
// conn == null, so churn is a warn-free map+alloc with no Redis. Warm to the
// allocator plateau first, then bound growth over the soak coarsely.
const HUB_RSS_WARMUP_CYCLES: usize = 32; // prime the smp_allocator plateau pre-baseline
const HUB_RSS_SOAK_ITERATIONS: usize = 16_384; // subscribe/unsubscribe rounds measured vs baseline
const HUB_RSS_GROWTH_BOUND_BYTES: u64 = 4 * 1024 * 1024; // 4 MiB — catches unbounded growth, not byte-exact

// ── Subscription mechanics (pure) ───────────────────────────────────────────

test "subscription: frames pop in publish order and ownership transfers" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.unref();

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
    defer sub.unref();

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
    defer sub.unref();
    try testing.expect(sub.pop(SHORT_POP_MS) == .timeout);
}

test "subscription: queued frames drain before the closed signal" {
    const sub = try Subscription.create(testing.allocator, common.globalIo(), CHANNEL_A);
    defer sub.unref();

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
    defer sub.unref();

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
    defer sub.unref();

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
    defer sub.unref();

    sub.close();
    sub.push(PAYLOAD_ONE);
    try testing.expect(sub.pop(SHORT_POP_MS) == .closed);
}

test "subscription: a failed payload copy counts a drop and delivers nothing" {
    var failing = std.testing.FailingAllocator.init(testing.allocator, .{ .fail_index = PUSH_COPY_FAIL_INDEX });
    const sub = try Subscription.create(failing.allocator(), common.globalIo(), CHANNEL_A);
    defer sub.unref();

    const dropped_before = metrics.snapshot().sse_dropped_frames_total;
    sub.push(PAYLOAD_ONE); // alloc.dupe fails → noteDrop, no enqueue
    try testing.expectEqual(@as(u64, 1), sub.dropCount());
    try testing.expectEqual(dropped_before + 1, metrics.snapshot().sse_dropped_frames_total);
    try testing.expect(sub.pop(SHORT_POP_MS) == .timeout);
}

// ── Shared consumer: N channels → ONE queue, each frame carrying its channel ─

test "subscription: a tagged frame carries its channel and the untouched payload" {
    const sub = try Subscription.createShared(testing.allocator, common.globalIo(), WS_LABEL);
    defer sub.unref();
    try testing.expect(sub.tagged);

    sub.pushTagged(CHANNEL_A, PAYLOAD_ONE);
    const got = sub.pop(SHORT_POP_MS);
    try testing.expect(got == .message);
    defer testing.allocator.free(got.message);

    const split = Subscription.splitTagged(got.message).?;
    try testing.expectEqualStrings(CHANNEL_A, split.channel_name);
    // the payload crosses the queue byte-for-byte — the tag is a prefix, not a rewrite
    try testing.expectEqualStrings(PAYLOAD_ONE, split.payload);
}

test "subscription: splitTagged rejects a frame with no channel delimiter" {
    // The multiplexed handler drops such a frame rather than mis-routing it.
    try testing.expect(Subscription.splitTagged(PAYLOAD_ONE) == null);
    try testing.expect(Subscription.splitTagged("") == null);
}

test "subscription: one shared queue interleaves frames from two channels in publish order" {
    const sub = try Subscription.createShared(testing.allocator, common.globalIo(), WS_LABEL);
    defer sub.unref();

    sub.pushTagged(CHANNEL_A, PAYLOAD_ONE);
    sub.pushTagged(CHANNEL_B, PAYLOAD_TWO);

    const first = sub.pop(SHORT_POP_MS);
    try testing.expect(first == .message);
    defer testing.allocator.free(first.message);
    const second = sub.pop(SHORT_POP_MS);
    try testing.expect(second == .message);
    defer testing.allocator.free(second.message);

    try testing.expectEqualStrings(CHANNEL_A, Subscription.splitTagged(first.message).?.channel_name);
    try testing.expectEqualStrings(CHANNEL_B, Subscription.splitTagged(second.message).?.channel_name);
}

test "subscription: the shared queue's memory bound is per-consumer, not per-channel" {
    // The fan-in's whole point: N attached channels share ONE ring, so a
    // stalled workspace stream costs QUEUE_CAPACITY frames total — never
    // capacity × fleets.
    const sub = try Subscription.createShared(testing.allocator, common.globalIo(), WS_LABEL);
    defer sub.unref();

    var i: usize = 0;
    while (i < Subscription.QUEUE_CAPACITY + 5) : (i += 1) {
        sub.pushTagged(if (i % 2 == 0) CHANNEL_A else CHANNEL_B, PAYLOAD_ONE);
    }
    try testing.expectEqual(@as(u64, 5), sub.dropCount());
    try testing.expectEqual(Subscription.QUEUE_CAPACITY, sub.count);
}

test "hub: a shared consumer attaches N channels and detaches each, leaving no channel behind" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    const shared = try hub.createSharedConsumer(WS_LABEL);
    try hub.attachChannel(shared, CHANNEL_A);
    try hub.attachChannel(shared, CHANNEL_B);
    // N channels, ONE consumer — the wire cost is per channel, the queue cost is one.
    try testing.expectEqual(@as(usize, 2), hub.channelCount());

    hub.detachChannel(shared, CHANNEL_A);
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
    hub.detachChannel(shared, CHANNEL_B);
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
    shared.unref();
}

test "hub: a shared consumer and a per-fleet subscriber share a channel by refcount" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    // The console's single-fleet stream and the wall's workspace stream watch
    // the same fleet: one wire SUBSCRIBE, two local consumers.
    const solo = try hub.subscribe(CHANNEL_A);
    const shared = try hub.createSharedConsumer(WS_LABEL);
    try hub.attachChannel(shared, CHANNEL_A);
    try testing.expectEqual(@as(usize, 1), hub.channelCount());

    hub.detachChannel(shared, CHANNEL_A);
    shared.unref();
    // the console's stream still holds the channel
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
    hub.unsubscribe(solo);
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}

test "hub: stop wakes a shared consumer parked on its fan-in" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();

    const shared = try hub.createSharedConsumer(WS_LABEL);
    try hub.attachChannel(shared, CHANNEL_A);
    try hub.attachChannel(shared, CHANNEL_B);

    var probe: WakeProbe = .{};
    const consumer = try std.Thread.spawn(.{}, popOnceIntoProbe, .{ shared, &probe });
    common.sleepNanos(PARK_SETTLE_NS);
    hub.stop(); // close-sweep reaches the consumer through EVERY channel it holds
    consumer.join();

    try testing.expect(probe.outcome == .closed);
    try testing.expect(probe.elapsed_ms < WAKE_BOUND_MS);

    hub.detachChannel(shared, CHANNEL_A);
    hub.detachChannel(shared, CHANNEL_B);
    shared.unref();
}

test "hub: the shared-consumer fan-in unwinds cleanly under allocation failure" {
    try std.testing.checkAllAllocationFailures(testing.allocator, sharedFanInRoundTrip, .{});
}

fn sharedFanInRoundTrip(alloc: std.mem.Allocator) !void {
    var hub = subscription_hub.init(alloc, common.globalIo());
    defer hub.deinit();
    const shared = try hub.createSharedConsumer(WS_LABEL);
    defer shared.unref();
    try hub.attachChannel(shared, CHANNEL_A);
    errdefer hub.detachChannel(shared, CHANNEL_A);
    try hub.attachChannel(shared, CHANNEL_B);
    hub.detachChannel(shared, CHANNEL_B);
    hub.detachChannel(shared, CHANNEL_A);
}

test "subscription: create unwinds cleanly under allocation failure" {
    try std.testing.checkAllAllocationFailures(testing.allocator, createDestroy, .{});
}

fn createDestroy(alloc: std.mem.Allocator) !void {
    const sub = try Subscription.create(alloc, common.globalIo(), CHANNEL_A);
    sub.unref();
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

test "hub: RSS growth over subscribe/unsubscribe churn stays bounded (cold hub, production allocator)" {
    // Skip early where no RSS reader exists — the probe can't run (never fail).
    if (common.rss.currentBytes() == null) return error.SkipZigTest;

    // Cold hub on the PRODUCTION general-purpose allocator (smp) — the
    // process-level layer testing.allocator's leak detector cannot see. No
    // Redis: conn == null makes wireSend a warn-free no-op, so each iteration
    // is a pure map insert/remove + Subscription alloc/free.
    var hub = subscription_hub.init(std.heap.smp_allocator, common.globalIo());
    defer hub.deinit();
    defer hub.stop();

    // Warm to the allocator plateau BEFORE reading the baseline.
    var w: usize = 0;
    while (w < HUB_RSS_WARMUP_CYCLES) : (w += 1) {
        const sub = try hub.subscribe(CHANNEL_A);
        hub.unsubscribe(sub);
    }

    const baseline = common.rss.currentBytes() orelse return error.SkipZigTest;
    var i: usize = 0;
    while (i < HUB_RSS_SOAK_ITERATIONS) : (i += 1) {
        const sub = try hub.subscribe(CHANNEL_A);
        hub.unsubscribe(sub);
    }
    const after = common.rss.currentBytes() orelse return error.SkipZigTest;

    // Balanced churn leaves the map empty — a residual entry would be a leak.
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
    // Saturating: RSS can dip below baseline as the allocator recycles pages.
    const growth = after -| baseline;
    try testing.expect(growth < HUB_RSS_GROWTH_BOUND_BYTES);
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

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();

    if (hub.start(cfg, try test_sched.start())) |_| return error.TestUnexpectedResult else |_| {}
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

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg, try test_sched.start());

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

test "integration: a shared consumer receives both channels over the wire, each frame tagged" {
    // The transport proof for the fan-in (RULE STR): frames must arrive on ONE
    // queue carrying the channel they were published on — asserted through a
    // real Redis PUBLISH, not by calling pushTagged directly.
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg, try test_sched.start());

    const shared = try hub.createSharedConsumer(WS_LABEL);
    try hub.attachChannel(shared, CHANNEL_A);
    try hub.attachChannel(shared, CHANNEL_B);
    defer {
        hub.detachChannel(shared, CHANNEL_A);
        hub.detachChannel(shared, CHANNEL_B);
        shared.unref();
    }
    // one wire subscriber per channel — the fan-in is a scoped SUBSCRIBE set,
    // never a pattern subscribe over every tenant's channels
    try expectNumsub(&pub_client, CHANNEL_A, 1);
    try expectNumsub(&pub_client, CHANNEL_B, 1);
    try testing.expectEqual(@as(usize, 2), hub.channelCount());

    try pub_client.publish(CHANNEL_A, PAYLOAD_ONE);
    try pub_client.publish(CHANNEL_B, PAYLOAD_TWO);

    // Both land on the ONE queue; the pub/sub order across two channels is not
    // guaranteed, so assert the SET of (channel, payload) pairs.
    var saw_a = false;
    var saw_b = false;
    for (0..2) |_| {
        const got = shared.pop(DELIVERY_WAIT_MS);
        try testing.expect(got == .message);
        defer testing.allocator.free(got.message);
        const split = Subscription.splitTagged(got.message).?;
        if (std.mem.eql(u8, split.channel_name, CHANNEL_A)) {
            try testing.expectEqualStrings(PAYLOAD_ONE, split.payload);
            saw_a = true;
        } else {
            try testing.expectEqualStrings(CHANNEL_B, split.channel_name);
            try testing.expectEqualStrings(PAYLOAD_TWO, split.payload);
            saw_b = true;
        }
    }
    try testing.expect(saw_a and saw_b);
}

test "integration: hub reconnects after its connection is killed and delivery resumes" {
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg, try test_sched.start());

    const sub = try hub.subscribe(CHANNEL_A);
    defer hub.unsubscribe(sub);
    try expectNumsub(&pub_client, CHANNEL_A, 1);

    // Sever only this hub's socket; never disrupt sibling Redis subscribers.
    const reconnects_before = metrics.snapshot().sse_hub_reconnects_total;
    try testing.expect(hub.testDisconnectConnection());

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

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg, try test_sched.start());

    const active = try hub.subscribe(CHANNEL_ISOLATION);
    defer hub.unsubscribe(active);
    const stalled = try hub.subscribe(CHANNEL_ISOLATION);
    defer hub.unsubscribe(stalled);
    // wait for the wire SUBSCRIBE to settle — frames published before the
    // server registers the subscription are lost, not queued
    try expectNumsub(&pub_client, CHANNEL_ISOLATION, 1);

    // The active sibling drains in lockstep (a live consumer keeps up) while
    // the stalled one never pops — its 64-slot ring must absorb all frames
    // and shed exactly the oldest 3. Draining active AFTER all publishes
    // would overflow ITS ring too; lockstep is the property's real shape.
    const total = Subscription.QUEUE_CAPACITY + 3;
    var frame_buf: [16]u8 = undefined;
    var i: usize = 0;
    while (i < total) : (i += 1) {
        const frame = try std.fmt.bufPrint(&frame_buf, "f{d}", .{i});
        try pub_client.publish(CHANNEL_ISOLATION, frame);
        const got = active.pop(DELIVERY_WAIT_MS);
        try testing.expect(got == .message);
        defer testing.allocator.free(got.message);
        try testing.expectEqualStrings(frame, got.message);
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

// ── Wire-write lock discipline (cold hub — protocol proofs, no Redis) ───────

/// Subscriber-thread body for the stalled-wire test: the FIRST subscribe on a
/// channel does a wire send, which blocks on the test-held wire lock.
const StalledFirstSubscribe = struct {
    fn run(hub: *subscription_hub, done: *common.Event, out_err: *?anyerror) void {
        const sub = hub.subscribe(CHANNEL_A) catch |err| {
            out_err.* = err;
            done.set();
            return;
        };
        done.set();
        // Leave detachment to the main thread's cleanup ordering.
        _ = sub;
    }
};

test "attach_with_stalled_peer_does_not_block_dispatch" {
    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();

    // Simulate a peer-stalled wire send: hold the wire lock so the first
    // subscriber's SUBSCRIBE send blocks exactly where a full send buffer
    // would block it.
    hub.testHoldWire();
    var wire_released = false;
    defer if (!wire_released) hub.testReleaseWire();

    var first_done = common.Event{};
    var first_err: ?anyerror = null;
    const first = try std.Thread.spawn(.{}, StalledFirstSubscribe.run, .{ &hub, &first_done, &first_err });
    defer first.join();

    // The first subscriber's map insert lands before its (blocked) wire send.
    var waited_ms: u64 = 0;
    while (hub.channelCount() == 0 and waited_ms < DELIVERY_WAIT_MS) : (waited_ms += 100) {
        common.sleepNanos(POLL_SLEEP_NS);
    }
    try testing.expectEqual(@as(usize, 1), hub.channelCount());

    // THE PROPERTY (pre-fix this deadlocked): with one wire send stalled,
    // every map operation still completes — a non-first subscribe, a
    // non-last unsubscribe, and the count.
    const second = try hub.subscribe(CHANNEL_A);
    try testing.expectEqual(@as(usize, 1), hub.channelCount());
    hub.unsubscribe(second);
    try testing.expect(!first_done.isSet());

    hub.testReleaseWire();
    wire_released = true;
    try first_done.timedWait(DELIVERY_WAIT_MS * std.time.ns_per_ms);
    try testing.expect(first_err == null);

    // Cleanup: stop closes the remaining subscription; drain it like a stream
    // thread would, then the count settles to zero.
    const remaining = blk: {
        hub.mutex.lockUncancelable(hub.io);
        defer hub.mutex.unlock(hub.io);
        const entry = hub.channels.get(CHANNEL_A) orelse break :blk null;
        break :blk entry.subscribers.items[0];
    };
    if (remaining) |sub| hub.unsubscribe(sub);
    hub.stop();
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}

test "hub_stop_undrained_no_conn_teardown_race" {
    // stop()'s drain-timeout warns by design; the build runner fails a
    // passing test that emits warn+ logs — silence for this test only.
    const saved_log_level = testing.log_level;
    testing.log_level = .err;
    defer testing.log_level = saved_log_level;

    var hub = subscription_hub.init(testing.allocator, common.globalIo());
    defer hub.deinit();
    // A wedged stream thread never detaches inside the bound: shrink the
    // bound so the test exercises the undrained path quickly.
    hub.stop_drain_max_ms = 100;

    const wedged = try hub.subscribe(CHANNEL_A);
    hub.stop(); // drain times out with the channel still live; conn teardown runs under the wire lock

    // The "wedged" stream thread finally unsubscribes AFTER stop returned —
    // pre-fix this read a connection stop() had already deinit'd (with a live
    // conn: use-after-free); post-fix the send path observes null under the
    // wire lock and skips.
    hub.unsubscribe(wedged);
    try testing.expectEqual(@as(usize, 0), hub.channelCount());
}

/// Churn-thread body for the concurrent read+write test: subscribe/unsubscribe
/// its own channel repeatedly, generating wire writes while the reader reads.
const WireChurn = struct {
    const ITERATIONS: usize = 25;
    fn run(hub: *subscription_hub, channel: []const u8, out_err: *?anyerror) void {
        var i: usize = 0;
        while (i < ITERATIONS) : (i += 1) {
            const sub = hub.subscribe(channel) catch |err| {
                out_err.* = err;
                return;
            };
            hub.unsubscribe(sub);
        }
    }
};

test "integration: concurrent_read_write_one_subscriber_conn" {
    const url = try requireRedisUrlOrSkip();
    const cfg = try queue_redis.testing.poolConfigFromUrl(testing.allocator, url);
    defer redis_config.deinitConfig(testing.allocator, cfg);
    var pub_client = try queue_redis.testing.connectFromUrl(common.globalIo(), testing.allocator, url);
    defer pub_client.deinit();

    var test_sched: TestScheduler = .{};
    defer test_sched.deinit();
    var hub = subscription_hub.init(testing.allocator, test_sched.io());
    defer hub.deinit();
    defer hub.stop();
    try hub.start(cfg, try test_sched.start());

    // A live viewer keeps the reader busy with real frames…
    const viewer = try hub.subscribe(CHANNEL_A);
    var viewer_detached = false;
    defer if (!viewer_detached) hub.unsubscribe(viewer);

    // …while churn threads hammer the write half of the SAME connection.
    var errs = [_]?anyerror{ null, null, null };
    const churn_channels = [_][]const u8{ "hubtest:churn:one", "hubtest:churn:two", "hubtest:churn:three" };
    var churn: [3]std.Thread = undefined;
    for (&churn, churn_channels, 0..) |*t, chan, i| {
        t.* = try std.Thread.spawn(.{}, WireChurn.run, .{ &hub, chan, &errs[i] });
    }

    // Publish through the churn window; every frame must reach the viewer —
    // interleaved reads and serialized writes on one connection corrupt
    // neither direction.
    var frame_buf: [16]u8 = undefined;
    var delivered: usize = 0;
    var i: usize = 0;
    while (i < 10) : (i += 1) {
        const frame = try std.fmt.bufPrint(&frame_buf, "rw{d}", .{i});
        try pub_client.publish(CHANNEL_A, frame);
        const got = viewer.pop(DELIVERY_WAIT_MS);
        if (got == .message) {
            testing.allocator.free(got.message);
            delivered += 1;
        }
    }
    for (&churn) |*t| t.join();

    for (errs) |maybe_err| try testing.expect(maybe_err == null);
    try testing.expectEqual(@as(usize, 10), delivered);
    hub.unsubscribe(viewer);
    viewer_detached = true;
    try expectNumsub(&pub_client, CHANNEL_A, 0);
}
