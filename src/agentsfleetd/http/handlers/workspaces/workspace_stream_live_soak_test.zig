const std = @import("std");
const common = @import("common");
const RedisClient = @import("../../../queue/redis.zig").Client;
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const SseClient = @import("../fleets/test_sse_client.zig");
const fx = @import("workspace_stream_test_fixtures.zig");
const metrics = @import("../../../observability/metrics.zig");
const sse_frame = @import("../sse_frame.zig");
const testing = std.testing;
const LIVE_CLIENTS: usize = 100;
const LIVE_STREAM_CAP: u32 = 128;
const LIVE_READ_DEADLINE_MS: u32 = 4_000;
const LIVE_WAIT_STEPS: usize = 50;
const LIVE_WAIT_NS: u64 = 100 * std.time.ns_per_ms;
const LIVE_SOAK_ENV = "LIVE_WORKSPACE_STREAM_SOAK";
const SCALE_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa101";
const RECOVERY_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa102";
const REFRESH_BASE_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa103";
const REFRESH_LATE_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa104";
const RSS_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa105";
const SHUTDOWN_FLEET = "0195b4ba-8d3a-7f13-8abc-2b3e1e0fa106";
const FLEET_CHURN_CLIENTS: usize = 12;
const WORKSPACE_EVENT_MARKER = "workspace-soak";
const LIVE_PAYLOAD = "{\"kind\":\"chunk\",\"event_id\":\"workspace-soak\",\"text\":\"x\"}";
const RECOVERY_BEFORE_MARKER = "recovery-before";
const RECOVERY_BEFORE_PAYLOAD = "{\"kind\":\"chunk\",\"event_id\":\"recovery-before\",\"text\":\"x\"}";
const RECOVERY_AFTER_MARKER = "recovery-after";
const RECOVERY_AFTER_PAYLOAD = "{\"kind\":\"chunk\",\"event_id\":\"recovery-after\",\"text\":\"x\"}";
const RSS_WARMUP_ROUNDS: usize = 2;
const RSS_MEASURE_ROUNDS: usize = 8;
const RSS_ROUND_CLIENTS: usize = 12;
const RSS_GROWTH_BOUND_BYTES: u64 = 4 * 1024 * 1024;
const SHUTDOWN_BOUND_MS: i64 = 5_000;

const ReadSlot = struct {
    client: *SseClient,
    ready: *std.atomic.Value(u32),
    go: *std.atomic.Value(bool),
    fleet_id: []const u8,
    received_ns: i128 = 0,
    failed: bool = false,
};

fn bootLive(alloc: std.mem.Allocator) !*TestHarness {
    const enabled = common.env.testLiveValue(LIVE_SOAK_ENV) orelse return error.SkipZigTest;
    if (!std.mem.eql(u8, enabled, "1")) return error.SkipZigTest;
    const h = fx.startHarnessWithWorkspace(alloc) catch |err| switch (err) {
        error.SkipZigTest, error.MissingRedisUrl => return error.SkipZigTest,
        else => return err,
    };
    h.ctx.sse_max_streams = LIVE_STREAM_CAP;
    return h;
}
fn seedLiveFleet(h: *TestHarness, fleet_id: []const u8, name: []const u8) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try fx.seedFleet(conn, fleet_id, name);
}
fn cleanupLiveFleet(h: *TestHarness, fleet_id: []const u8) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    fx.cleanupFleet(conn, fleet_id);
}
fn openLiveClient(alloc: std.mem.Allocator, h: *TestHarness, path: []const u8) !SseClient {
    return SseClient.connect(alloc, h.port, path, .{
        .bearer = fx.TOKEN_OPERATOR,
        .deadline_ms = LIVE_READ_DEADLINE_MS,
    });
}
fn readFleetFrame(slot: *ReadSlot) void {
    // safe because: ready only counts arrivals; ordering comes from go.
    _ = slot.ready.fetchAdd(1, .monotonic);
    // safe because: acquire pairs with the publisher's release below.
    while (!slot.go.load(.acquire)) common.sleepNanos(std.time.ns_per_ms);
    while (true) {
        var frame = slot.client.nextFrame() catch {
            slot.failed = true;
            return;
        };
        if (std.mem.indexOf(u8, frame.data, slot.fleet_id) == null) {
            frame.deinit(slot.client.alloc);
            continue;
        }
        if (std.mem.indexOf(u8, frame.data, WORKSPACE_EVENT_MARKER) == null) {
            frame.deinit(slot.client.alloc);
            slot.failed = true;
            return;
        }
        slot.received_ns = common.clock.nowNanos();
        frame.deinit(slot.client.alloc);
        return;
    }
}
fn waitForCount(h: *TestHarness, expected: usize) !void {
    for (0..LIVE_WAIT_STEPS) |_| {
        if (h.streams.count() == expected) return;
        common.sleepNanos(LIVE_WAIT_NS);
    }
    return error.StreamCountTimeout;
}
fn percentile95(values: []u64) u64 {
    std.mem.sort(u64, values, {}, std.sort.asc(u64));
    return values[(values.len - 1) * 95 / 100];
}
test "integration: one hundred workspace streams receive one fleet-tagged publish within the latency budget" {
    const h = try bootLive(testing.allocator);
    defer h.deinit();
    try seedLiveFleet(h, SCALE_FLEET, "workspace-scale");
    defer cleanupLiveFleet(h, SCALE_FLEET);
    var publisher = fx.connectPublisher(testing.allocator) catch return error.SkipZigTest;
    defer publisher.deinit();
    const channel = try fx.activityChannel(testing.allocator, SCALE_FLEET);
    defer testing.allocator.free(channel);
    const path = try fx.workspaceStreamPath(testing.allocator);
    defer testing.allocator.free(path);

    var clients: [LIVE_CLIENTS]SseClient = undefined;
    var opened: usize = 0;
    defer for (clients[0..opened]) |*client| client.deinit();
    while (opened < LIVE_CLIENTS) : (opened += 1) clients[opened] = try openLiveClient(testing.allocator, h, path);
    try waitForCount(h, LIVE_CLIENTS);
    for (&clients) |*client| try consumeInitialHello(client);
    common.sleepNanos(fx.SUBSCRIBE_SETTLE_NS);

    var ready: std.atomic.Value(u32) = .init(0);
    var go: std.atomic.Value(bool) = .init(false);
    var slots: [LIVE_CLIENTS]ReadSlot = undefined;
    var threads: [LIVE_CLIENTS]std.Thread = undefined;
    var spawned: usize = 0;
    defer {
        go.store(true, .release);
        for (threads[0..spawned]) |thread| thread.join();
    }
    while (spawned < LIVE_CLIENTS) : (spawned += 1) {
        slots[spawned] = .{ .client = &clients[spawned], .ready = &ready, .go = &go, .fleet_id = SCALE_FLEET };
        threads[spawned] = try std.Thread.spawn(.{}, readFleetFrame, .{&slots[spawned]});
    }
    // safe because: ready is only a barrier count, not a data publication.
    while (ready.load(.monotonic) != LIVE_CLIENTS) common.sleepNanos(std.time.ns_per_ms);
    go.store(true, .release); // safe because: reader acquire starts every read after this point.
    const published_ns = common.clock.nowNanos();
    try publisher.publish(channel, LIVE_PAYLOAD);
    for (threads[0..spawned]) |thread| thread.join();
    spawned = 0;

    var latencies: [LIVE_CLIENTS]u64 = undefined;
    for (&slots, 0..) |slot, i| {
        try testing.expect(!slot.failed);
        latencies[i] = @intCast(@divTrunc(slot.received_ns - published_ns, std.time.ns_per_ms));
    }
    try testing.expect(percentile95(&latencies) < 200);
    try testing.expectEqual(@as(usize, LIVE_CLIENTS), h.streams.count());
}
fn awaitFleetFrame(client: *SseClient, publisher: anytype, channel: []const u8, fleet_id: []const u8, payload: []const u8, marker: []const u8) !void {
    for (0..LIVE_WAIT_STEPS) |_| {
        try publisher.publish(channel, payload);
        var frame = client.nextFrame() catch |err| switch (err) {
            error.SseFrameTimeout => continue,
            else => return err,
        };
        if (std.mem.indexOf(u8, frame.data, fleet_id) != null and std.mem.indexOf(u8, frame.data, marker) != null) {
            frame.deinit(client.alloc);
            return;
        }
        frame.deinit(client.alloc);
    }
    return error.FleetFrameTimeout;
}
fn consumeInitialHello(client: *SseClient) !void {
    for (0..LIVE_WAIT_STEPS) |_| {
        var frame = try client.nextFrame();
        defer frame.deinit(client.alloc);
        if (std.mem.eql(u8, frame.event, sse_frame.KIND_HELLO)) return;
    }
    return error.HelloFrameTimeout;
}
fn expectClosed(client: *SseClient) !void {
    for (0..LIVE_WAIT_STEPS) |_| {
        var frame = client.nextFrame() catch |err| switch (err) {
            error.EndOfStream => return,
            else => return err,
        };
        frame.deinit(client.alloc);
    }
    return error.StreamDidNotClose;
}
test "integration: workspace stream survives Redis pubsub termination and resumes without client reconnect" {
    const h = try bootLive(testing.allocator);
    defer h.deinit();
    try seedLiveFleet(h, RECOVERY_FLEET, "workspace-recovery");
    defer cleanupLiveFleet(h, RECOVERY_FLEET);
    var publisher = fx.connectPublisher(testing.allocator) catch return error.SkipZigTest;
    defer publisher.deinit();
    const channel = try fx.activityChannel(testing.allocator, RECOVERY_FLEET);
    defer testing.allocator.free(channel);
    const path = try fx.workspaceStreamPath(testing.allocator);
    defer testing.allocator.free(path);
    var client = try openLiveClient(testing.allocator, h, path);
    defer client.deinit();
    common.sleepNanos(fx.SUBSCRIBE_SETTLE_NS);
    try awaitFleetFrame(&client, &publisher, channel, RECOVERY_FLEET, RECOVERY_BEFORE_PAYLOAD, RECOVERY_BEFORE_MARKER);

    const reconnects_before = metrics.snapshot().sse_hub_reconnects_total;
    var reply = try publisher.command(&.{ "CLIENT", "KILL", "TYPE", "pubsub" });
    reply.deinit(testing.allocator);
    try waitForSubscriberCount(&publisher, channel, 1);
    try awaitFleetFrame(&client, &publisher, channel, RECOVERY_FLEET, RECOVERY_AFTER_PAYLOAD, RECOVERY_AFTER_MARKER);
    try testing.expectEqual(@as(usize, 1), h.streams.count());
    try testing.expect(metrics.snapshot().sse_hub_reconnects_total >= reconnects_before + 1);
}
fn waitForSubscriberCount(publisher: anytype, channel: []const u8, expected: i64) !void {
    for (0..LIVE_WAIT_STEPS) |_| {
        var reply = try publisher.command(&.{ "PUBSUB", "NUMSUB", channel });
        defer reply.deinit(testing.allocator);
        if (reply != .array) return error.InvalidSubscriberCount;
        const values = reply.array orelse return error.InvalidSubscriberCount;
        if (values.len < 2 or values[1] != .integer) return error.InvalidSubscriberCount;
        if (values[1].integer == expected) return;
        common.sleepNanos(LIVE_WAIT_NS);
    }
    return error.SubscriberCountTimeout;
}
test "integration: live workspace streams refresh added and deleted PostgreSQL fleets without orphan channels" {
    const h = try bootLive(testing.allocator);
    defer h.deinit();
    try seedLiveFleet(h, REFRESH_BASE_FLEET, "workspace-refresh-base");
    defer cleanupLiveFleet(h, REFRESH_BASE_FLEET);
    var publisher = fx.connectPublisher(testing.allocator) catch return error.SkipZigTest;
    defer publisher.deinit();
    const base_channel = try fx.activityChannel(testing.allocator, REFRESH_BASE_FLEET);
    defer testing.allocator.free(base_channel);
    const late_channel = try fx.activityChannel(testing.allocator, REFRESH_LATE_FLEET);
    defer testing.allocator.free(late_channel);
    const path = try fx.workspaceStreamPath(testing.allocator);
    defer testing.allocator.free(path);
    var clients: [FLEET_CHURN_CLIENTS]SseClient = undefined;
    var opened: usize = 0;
    defer for (clients[0..opened]) |*client| client.deinit();
    while (opened < clients.len) : (opened += 1) clients[opened] = try openLiveClient(testing.allocator, h, path);
    try waitForCount(h, clients.len);
    try waitForSubscriberCount(&publisher, base_channel, 1);
    try awaitFleetFrame(&clients[0], &publisher, base_channel, REFRESH_BASE_FLEET, LIVE_PAYLOAD, WORKSPACE_EVENT_MARKER);

    try seedLiveFleet(h, REFRESH_LATE_FLEET, "workspace-refresh-late");
    defer cleanupLiveFleet(h, REFRESH_LATE_FLEET);
    try waitForSubscriberCount(&publisher, late_channel, 1);
    try awaitFleetFrame(&clients[0], &publisher, late_channel, REFRESH_LATE_FLEET, LIVE_PAYLOAD, WORKSPACE_EVENT_MARKER);

    cleanupLiveFleet(h, REFRESH_LATE_FLEET);
    try waitForSubscriberCount(&publisher, late_channel, 0);
    try awaitFleetFrame(&clients[0], &publisher, base_channel, REFRESH_BASE_FLEET, LIVE_PAYLOAD, WORKSPACE_EVENT_MARKER);
    try waitForSubscriberCount(&publisher, base_channel, 1);
}
fn churnStreams(alloc: std.mem.Allocator, h: *TestHarness, publisher: anytype, channel: []const u8, path: []const u8, rounds: usize) !void {
    for (0..rounds) |_| {
        var clients: [RSS_ROUND_CLIENTS]SseClient = undefined;
        var opened: usize = 0;
        defer for (clients[0..opened]) |*client| client.deinit();
        while (opened < clients.len) : (opened += 1) clients[opened] = try openLiveClient(alloc, h, path);
        try waitForCount(h, clients.len);
        for (&clients) |*client| client.closeStream();
        try publisher.publish(channel, LIVE_PAYLOAD);
        try waitForCount(h, 0);
        for (&clients) |*client| client.deinit();
        opened = 0;
    }
}
test "integration: production allocator RSS stays bounded across real workspace stream churn" {
    if (common.rss.currentBytes() == null) return error.SkipZigTest;
    const alloc = std.heap.smp_allocator;
    const h = try bootLive(alloc);
    defer h.deinit();
    try seedLiveFleet(h, RSS_FLEET, "workspace-rss");
    defer cleanupLiveFleet(h, RSS_FLEET);
    var publisher = fx.connectPublisher(alloc) catch return error.SkipZigTest;
    defer publisher.deinit();
    const channel = try fx.activityChannel(alloc, RSS_FLEET);
    defer alloc.free(channel);
    const path = try fx.workspaceStreamPath(alloc);
    defer alloc.free(path);

    try churnStreams(alloc, h, &publisher, channel, path, RSS_WARMUP_ROUNDS);
    const baseline = common.rss.currentBytes() orelse return error.SkipZigTest;
    try churnStreams(alloc, h, &publisher, channel, path, RSS_MEASURE_ROUNDS);
    const after = common.rss.currentBytes() orelse return error.SkipZigTest;
    try testing.expectEqual(@as(usize, 0), h.streams.count());
    try testing.expect(after -| baseline < RSS_GROWTH_BOUND_BYTES);
}
fn publishUntilStopped(client: *RedisClient, channel: []const u8, stop: *std.atomic.Value(bool), published: *std.atomic.Value(u32), failed: *std.atomic.Value(bool)) void {
    while (!stop.load(.acquire)) {
        client.publish(channel, LIVE_PAYLOAD) catch {
            failed.store(true, .release);
            return;
        };
        _ = published.fetchAdd(1, .release);
        common.sleepNanos(std.time.ns_per_ms);
    }
}

test "integration: draining one hundred workspace streams closes every client and clears the registry" {
    const h = try bootLive(testing.allocator);
    defer h.deinit();
    try seedLiveFleet(h, SHUTDOWN_FLEET, "workspace-shutdown");
    defer cleanupLiveFleet(h, SHUTDOWN_FLEET);
    const path = try fx.workspaceStreamPath(testing.allocator);
    defer testing.allocator.free(path);
    const channel = try fx.activityChannel(testing.allocator, SHUTDOWN_FLEET);
    defer testing.allocator.free(channel);
    var publisher = fx.connectPublisher(testing.allocator) catch return error.SkipZigTest;
    defer publisher.deinit();
    var clients: [LIVE_CLIENTS]SseClient = undefined;
    var opened: usize = 0;
    defer for (clients[0..opened]) |*client| client.deinit();
    while (opened < clients.len) : (opened += 1) clients[opened] = try openLiveClient(testing.allocator, h, path);
    try waitForCount(h, LIVE_CLIENTS);
    for (&clients) |*client| try consumeInitialHello(client);
    var stop: std.atomic.Value(bool) = .init(false);
    var published: std.atomic.Value(u32) = .init(0);
    var publish_failed: std.atomic.Value(bool) = .init(false);
    const publish_thread = try std.Thread.spawn(.{}, publishUntilStopped, .{ &publisher, channel, &stop, &published, &publish_failed });
    var publish_joined = false;
    defer {
        stop.store(true, .release);
        if (!publish_joined) publish_thread.join();
    }
    for (0..LIVE_WAIT_STEPS) |_| {
        if (published.load(.acquire) > 0 or publish_failed.load(.acquire)) break;
        common.sleepNanos(LIVE_WAIT_NS);
    }
    try testing.expect(published.load(.acquire) > 0);
    try testing.expect(!publish_failed.load(.acquire));

    const started = common.clock.nowMillis();
    h.streams.drain();
    h.hub.stop();
    h.streams.awaitEmpty();
    stop.store(true, .release);
    publish_thread.join();
    publish_joined = true;
    try testing.expect(!publish_failed.load(.acquire));
    try testing.expect(common.clock.nowMillis() - started < SHUTDOWN_BOUND_MS);
    try testing.expectEqual(@as(usize, 0), h.streams.count());
    for (&clients) |*client| {
        try expectClosed(client);
        client.deinit();
    }
    opened = 0;
}
