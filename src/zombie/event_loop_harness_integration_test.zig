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
const EventEnvelope = @import("event_envelope.zig");
const base = @import("../db/test_fixtures.zig");
const activity_publisher = @import("activity_publisher.zig");
const Harness = @import("test_executor_harness.zig");
const helpers = @import("test_harness_helpers.zig");

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f20";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa20";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa21";
const ACTIVITY_CHANNEL = "zombie:" ++ TEST_ZOMBIE_ID ++ ":activity";
const TEST_ACTOR = "steer:test-user";
const TEST_REQUEST_JSON = "{\"message\":\"ping\"}";
const EMPTY_CONTEXT_JSON = "{}";
const TEST_CONSUMER = "harness-test-consumer";

const DRAIN_OVERALL_BUDGET_MS: u64 = 8_000;

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

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

test "integration: harness emits started/chunk/completed frames in order via real worker pipeline" {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // The default RedisRole reads REDIS_URL, but integration tests run with
    // TEST_REDIS_TLS_URL set instead — connect via URL directly.
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

    var harness = try Harness.start(ALLOC, .{ .script_json = PROGRESS_SCRIPT });
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
    var consec: u32 = 0;
    const next_consec = writepath.run(ALLOC, &session, &evt, cfg, &consec);
    try std.testing.expectEqual(@as(u32, 0), next_consec);

    var frames = helpers.FrameList{};
    defer helpers.freeFrames(ALLOC, &frames);
    try helpers.drainFrames(&sub, ALLOC, &frames, DRAIN_OVERALL_BUDGET_MS);

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
    try std.testing.expectEqual(expected.len, frames.items.len);
    for (expected, frames.items) |want, got| {
        try std.testing.expectEqualStrings(want, got.kind);
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
