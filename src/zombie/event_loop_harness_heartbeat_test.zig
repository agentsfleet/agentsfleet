// Heartbeat-cadence test for the worker write path against the executor
// harness. Scripts the harness with 3× tool_call_progress frames, each
// preceded by a 2 s sleep, and asserts the worker's PUBLISH side observes
// them at ~2 s intervals — the same shape the SSE handler exposes to the
// dashboard's "tool still running" spinner.
//
// Same env requirements as event_loop_harness_integration_test.zig.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const event_loop = @import("event_loop.zig");
const writepath = @import("event_loop_writepath.zig");
const types = @import("event_loop_types.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const redis_pubsub = @import("../queue/redis_pubsub.zig");
const EventEnvelope = @import("event_envelope.zig");
const base = @import("../db/test_fixtures.zig");
const activity_publisher = @import("activity_publisher.zig");
const Harness = @import("test_executor_harness.zig");
const helpers = @import("test_harness_helpers.zig");

const ALLOC = std.testing.allocator;

// Distinct workspace + zombie ids so a flaky teardown in the
// progress-callbacks test can't bleed state into this one.
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f30";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa30";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa31";
const ACTIVITY_CHANNEL = "zombie:" ++ TEST_ZOMBIE_ID ++ ":activity";
const TEST_ACTOR = "steer:test-user";
const TEST_REQUEST_JSON = "{\"message\":\"long-tool\"}";
const EMPTY_CONTEXT_JSON = "{}";
const TEST_CONSUMER = "harness-heartbeat-consumer";

// 3× tool_call_progress, each preceded by a 2 s sleep — exercises the
// "long tool call still alive" heartbeat path. Total wall ≈ 6 s.
const HEARTBEAT_SCRIPT =
    \\{"frames":[
    \\  {"kind":"tool_call_progress","name":"http.request","elapsed_ms":2000,"delay_before_ms":2000},
    \\  {"kind":"tool_call_progress","name":"http.request","elapsed_ms":4000,"delay_before_ms":2000},
    \\  {"kind":"tool_call_progress","name":"http.request","elapsed_ms":6000,"delay_before_ms":2000}
    \\],"result":{"content":"ok","exit_ok":true}}
;

// Each progress frame should land at the subscriber roughly 2 s after the
// prior one. Allow a ±1.5 s window to absorb RPC round-trip + scheduler
// jitter on busy CI runners.
const HEARTBEAT_INTERVAL_MIN_MS: i64 = 500;
const HEARTBEAT_INTERVAL_MAX_MS: i64 = 3_500;
const HEARTBEAT_DRAIN_BUDGET_MS: u64 = 15_000;
const EXPECTED_PROGRESS_FRAMES: usize = 3;

// Drain runs on a background thread so it samples readMessage() in real
// time while writepath.run blocks the main thread for ~6s. Without
// concurrency, all 3 frames buffer in the kernel/Redis pipeline and
// collapse to ms-level intervals when the test thread drains afterwards.
const DrainCtx = struct {
    sub: *redis_pubsub.Subscriber,
    alloc: Allocator,
    frames: *helpers.FrameList,
    budget_ms: u64,
    err: ?anyerror = null,
};

fn drainThread(ctx: *DrainCtx) void {
    helpers.drainFrames(ctx.sub, ctx.alloc, ctx.frames, ctx.budget_ms) catch |e| {
        ctx.err = e;
    };
}

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

test "integration: harness emits ≥3 tool_call_progress frames at ~2s intervals" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = blk: {
        const tls_url = std.process.getEnvVarOwned(ALLOC, helpers.REDIS_TLS_URL_ENV_VAR) catch return error.SkipZigTest;
        defer ALLOC.free(tls_url);
        break :blk queue_redis.Client.connectFromUrl(ALLOC, tls_url) catch return error.SkipZigTest;
    };
    defer redis.deinit();

    var sub = (try helpers.connectSubscriber(ALLOC)) orelse return error.SkipZigTest;
    defer sub.deinit();
    try sub.subscribe(ACTIVITY_CHANNEL);
    std.Thread.sleep(helpers.SUBSCRIBE_SETTLE_MS * std.time.ns_per_ms);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, helpers.ZOMBIE_NAME, helpers.ZOMBIE_CONFIG_JSON, helpers.ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, EMPTY_CONTEXT_JSON);

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);
    defer cleanupZombieEventsRows(db_ctx.conn);

    try redis_zombie.ensureZombieConsumerGroup(&redis, TEST_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .script_json = HEARTBEAT_SCRIPT });
    defer harness.deinit();

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = TEST_ZOMBIE_ID,
        .workspace_id = TEST_WORKSPACE_ID,
        .actor = TEST_ACTOR,
        .event_type = .chat,
        .request_json = TEST_REQUEST_JSON,
        .created_at = std.time.milliTimestamp(),
    };
    const xadded_id = try redis.xaddZombieEvent(envelope);
    defer ALLOC.free(xadded_id);

    var evt = (try redis_zombie.xreadgroupZombie(&redis, TEST_ZOMBIE_ID, TEST_CONSUMER)) orelse
        return error.UnexpectedNullEvent;
    defer evt.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .redis_publish = &redis,
        .executor = harness.executorPtr(),
        .running = &running,
        .balance_policy = .stop,
    };
    var frames = helpers.FrameList{};
    defer helpers.freeFrames(ALLOC, &frames);

    var drain_ctx = DrainCtx{
        .sub = &sub,
        .alloc = ALLOC,
        .frames = &frames,
        .budget_ms = HEARTBEAT_DRAIN_BUDGET_MS,
    };
    const drainer = try std.Thread.spawn(.{}, drainThread, .{&drain_ctx});

    var consec: u32 = 0;
    const next_consec = writepath.run(ALLOC, &session, &evt, cfg, &consec);
    try std.testing.expectEqual(@as(u32, 0), next_consec);

    drainer.join();
    if (drain_ctx.err) |e| return e;

    // Filter to just the heartbeat frames; cadence assertion is over the
    // worker's PUBLISH timestamps, not interleaved with received/complete.
    var progress_ts = std.ArrayList(i64){};
    defer progress_ts.deinit(ALLOC);
    for (frames.items) |f| {
        if (std.mem.eql(u8, f.kind, activity_publisher.KIND_TOOL_CALL_PROGRESS)) {
            try progress_ts.append(ALLOC, f.ts_ms);
        }
    }

    try std.testing.expect(progress_ts.items.len >= EXPECTED_PROGRESS_FRAMES);

    // Inter-frame intervals: each adjacent pair should fall in the budget.
    var i: usize = 1;
    while (i < progress_ts.items.len) : (i += 1) {
        const delta = progress_ts.items[i] - progress_ts.items[i - 1];
        try std.testing.expect(delta >= HEARTBEAT_INTERVAL_MIN_MS);
        try std.testing.expect(delta <= HEARTBEAT_INTERVAL_MAX_MS);
    }
}
