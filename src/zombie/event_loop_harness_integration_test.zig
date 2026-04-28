// Integration tests for the worker write path against the executor harness
// binary. The harness emits scripted progress frames (see runner_harness.zig);
// the worker should ingest them via its real ExecutorClient + ProgressEmitter
// wiring and PUBLISH them onto `zombie:{id}:activity`.
//
// Requires:
//   - LIVE_DB=1 (Postgres reachable via TEST_DATABASE_URL / DATABASE_URL).
//   - TEST_REDIS_TLS_URL pointing at a reachable Redis.
//   - `zig-out/bin/zombied-executor-harness` present (test step depends on it).
// Skipped otherwise.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
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

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f20";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa20";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa21";
const ACTIVITY_CHANNEL = "zombie:" ++ TEST_ZOMBIE_ID ++ ":activity";

const DRAIN_QUIET_MS: u64 = 250;
const DRAIN_OVERALL_BUDGET_MS: u64 = 8_000;
const SUBSCRIBE_SETTLE_MS: u64 = 200;

const VALID_CONFIG_JSON =
    \\{"name":"harness-bot","trigger":{"type":"webhook","source":"agentmail"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: harness-bot
    \\---
    \\
    \\You are a harness bot.
;

const PROGRESS_SCRIPT =
    \\{"frames":[
    \\  {"kind":"tool_call_started","name":"http.request","args_redacted":"{\"url\":\"https://example.com\"}"},
    \\  {"kind":"tool_call_started","name":"file.read","args_redacted":"{\"path\":\"/etc/hosts\"}"},
    \\  {"kind":"agent_response_chunk","text":"Hello"},
    \\  {"kind":"agent_response_chunk","text":" "},
    \\  {"kind":"agent_response_chunk","text":"world"},
    \\  {"kind":"agent_response_chunk","text":"!"},
    \\  {"kind":"agent_response_chunk","text":"\n"},
    \\  {"kind":"tool_call_completed","name":"http.request","ms":120},
    \\  {"kind":"tool_call_completed","name":"file.read","ms":80}
    \\],"result":{"content":"ok","exit_ok":true}}
;

fn connectSubscriber(alloc: Allocator) !?redis_pubsub.Subscriber {
    const tls_url = std.process.getEnvVarOwned(alloc, "TEST_REDIS_TLS_URL") catch return null;
    defer alloc.free(tls_url);
    return redis_pubsub.Subscriber.connectFromUrl(alloc, tls_url) catch return null;
}

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

const FrameKindList = std.ArrayList([]u8); // BUFFER GATE: ArrayList for kinds — appendable + indexable for ordered assertions.

fn drainKinds(sub: *redis_pubsub.Subscriber, alloc: Allocator, list: *FrameKindList) !void {
    const overall_deadline = std.time.milliTimestamp() + @as(i64, @intCast(DRAIN_OVERALL_BUDGET_MS));
    var last_msg_at = std.time.milliTimestamp();
    while (std.time.milliTimestamp() < overall_deadline) {
        const msg_opt = try sub.readMessage();
        if (msg_opt) |m| {
            var msg = m;
            defer msg.deinit();
            const kind = try extractKind(alloc, msg.data);
            try list.append(alloc, kind);
            last_msg_at = std.time.milliTimestamp();
            // event_complete is the terminal marker — stop draining once it lands.
            if (std.mem.eql(u8, kind, "event_complete")) return;
            continue;
        }
        if (std.time.milliTimestamp() - last_msg_at > @as(i64, @intCast(DRAIN_QUIET_MS))) return;
    }
}

fn extractKind(alloc: Allocator, payload: []const u8) ![]u8 {
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, payload, .{});
    defer parsed.deinit();
    const root = parsed.value;
    if (root != .object) return error.MissingKind;
    const v = root.object.get("kind") orelse return error.MissingKind;
    if (v != .string) return error.MissingKind;
    return alloc.dupe(u8, v.string);
}

fn freeKinds(alloc: Allocator, list: *FrameKindList) void {
    for (list.items) |k| alloc.free(k);
    list.deinit(alloc);
}

test "integration: harness emits started/chunk/completed frames in order via real worker pipeline" {
    if (std.process.getEnvVarOwned(ALLOC, "EXECUTOR_HARNESS_SKIP")) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // The default RedisRole reads REDIS_URL, but integration tests run with
    // TEST_REDIS_TLS_URL set instead — connect via URL directly.
    var redis = blk: {
        const tls_url = std.process.getEnvVarOwned(ALLOC, "TEST_REDIS_TLS_URL") catch return error.SkipZigTest;
        defer ALLOC.free(tls_url);
        break :blk queue_redis.Client.connectFromUrl(ALLOC, tls_url) catch return error.SkipZigTest;
    };
    defer redis.deinit();

    var sub = (try connectSubscriber(ALLOC)) orelse return error.SkipZigTest;
    defer sub.deinit();
    try sub.subscribe(ACTIVITY_CHANNEL);
    std.Thread.sleep(SUBSCRIBE_SETTLE_MS * std.time.ns_per_ms);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "harness-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);
    defer cleanupZombieEventsRows(db_ctx.conn);

    try redis_zombie.ensureZombieConsumerGroup(&redis, TEST_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .script_json = PROGRESS_SCRIPT });
    defer harness.deinit();

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = TEST_ZOMBIE_ID,
        .workspace_id = TEST_WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = std.time.milliTimestamp(),
    };
    const xadded_id = try redis.xaddZombieEvent(envelope);
    defer ALLOC.free(xadded_id);

    var evt = (try redis_zombie.xreadgroupZombie(&redis, TEST_ZOMBIE_ID, "harness-test-consumer")) orelse
        return error.UnexpectedNullEvent;
    defer evt.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .executor = harness.executorPtr(),
        .running = &running,
        .balance_policy = .stop,
    };
    var consec: u32 = 0;
    const next_consec = writepath.run(ALLOC, &session, &evt, cfg, &consec);
    try std.testing.expectEqual(@as(u32, 0), next_consec);

    var kinds = FrameKindList{};
    defer freeKinds(ALLOC, &kinds);
    try drainKinds(&sub, ALLOC, &kinds);

    // Expected sequence: event_received, 2× tool_call_started, 5× chunk,
    // 2× tool_call_completed, event_complete. Strict order — the worker
    // PUBLISHes synchronously per frame received from the executor RPC.
    const expected = [_][]const u8{
        activity_publisher.KIND_EVENT_RECEIVED,
        activity_publisher.KIND_TOOL_CALL_STARTED,
        activity_publisher.KIND_TOOL_CALL_STARTED,
        activity_publisher.KIND_CHUNK,
        activity_publisher.KIND_CHUNK,
        activity_publisher.KIND_CHUNK,
        activity_publisher.KIND_CHUNK,
        activity_publisher.KIND_CHUNK,
        activity_publisher.KIND_TOOL_CALL_COMPLETED,
        activity_publisher.KIND_TOOL_CALL_COMPLETED,
        activity_publisher.KIND_EVENT_COMPLETE,
    };
    try std.testing.expectEqual(expected.len, kinds.items.len);
    for (expected, kinds.items) |want, got| {
        try std.testing.expectEqualStrings(want, got);
    }

    // Verify zombie_events row landed status=processed (terminal).
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT status FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ TEST_ZOMBIE_ID, evt.event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("processed", try row.get([]const u8, 0));
}
