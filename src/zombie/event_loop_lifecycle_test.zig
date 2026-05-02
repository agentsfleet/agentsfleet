// Worker write-path lifecycle tests against the executor harness.
//
// Covers the spec's "three Postgres rows per processed event, one join
// key" invariant: every successfully processed event lands exactly one
// new row in core.zombie_events (status=processed), one row in
// zombie_execution_telemetry (write-once), and one mutated row in
// core.zombie_sessions — all keyed by the same event_id. A replay (the
// worker dies between INSERT received and XACK; M40's XAUTOCLAIM
// redelivers the same Redis-stream entry) must not duplicate any of
// the three.
//
// Same env requirements as event_loop_harness_integration_test.zig.

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
const Harness = @import("test_executor_harness.zig");
const helpers = @import("test_harness_helpers.zig");

const ALLOC = std.testing.allocator;

// Distinct workspace + zombie ids per test so concurrent runs / flaky
// teardown don't bleed state between cases.
const LC_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f50";
const LC_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa50";
const LC_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa51";

const RP_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f51";
const RP_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa55";
const RP_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa56";

const TEST_ACTOR = "steer:test-user";
const TEST_REQUEST_JSON = "{\"message\":\"lifecycle\"}";
const EMPTY_CONTEXT_JSON = "{}";
const STATUS_PROCESSED = "processed";

const LIFECYCLE_SCRIPT =
    \\{"frames":[
    \\  {"kind":"agent_response_chunk","text":"hello"}
    \\],"result":{"content":"ok","exit_ok":true}}
;

fn deleteEventStream(redis: *queue_redis.Client, zombie_id: []const u8) void {
    const key = std.fmt.allocPrint(redis.alloc, "zombie:{s}:events", .{zombie_id}) catch return;
    defer redis.alloc.free(key);
    var resp = redis.command(&.{ "DEL", key }) catch return;
    defer resp.deinit(redis.alloc);
}

fn cleanupEventRows(conn: *pg.Conn, zombie_id: []const u8) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{zombie_id}) catch {};
}

fn cleanupTelemetry(conn: *pg.Conn, zombie_id: []const u8) void {
    // zombie_execution_telemetry.zombie_id is TEXT (not UUID), so no cast.
    _ = conn.exec("DELETE FROM zombie_execution_telemetry WHERE zombie_id = $1", .{zombie_id}) catch {};
}

fn openRedis(alloc: Allocator) !?queue_redis.Client {
    const tls_url = std.process.getEnvVarOwned(alloc, helpers.REDIS_TLS_URL_ENV_VAR) catch return null;
    defer alloc.free(tls_url);
    return queue_redis.Client.connectFromUrl(alloc, tls_url) catch return null;
}

fn skipIfHarnessDisabled() !void {
    if (std.process.getEnvVarOwned(ALLOC, helpers.SKIP_ENV_VAR)) |s| {
        defer ALLOC.free(s);
        return error.SkipZigTest;
    } else |_| {}
}

test "integration: one processed event lands 1 zombie_events + 2 telemetry (receive+stage) + 1 zombie_sessions row" {
    try skipIfHarnessDisabled();

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = (try openRedis(ALLOC)) orelse return error.SkipZigTest;
    defer redis.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, LC_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, LC_WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, LC_WORKSPACE_ID);
    defer base.teardownPlatformProvider(db_ctx.conn, LC_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, LC_ZOMBIE_ID, LC_WORKSPACE_ID, helpers.ZOMBIE_NAME, helpers.ZOMBIE_CONFIG_JSON, helpers.ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, LC_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, LC_SESSION_ID, LC_ZOMBIE_ID, EMPTY_CONTEXT_JSON);

    deleteEventStream(&redis, LC_ZOMBIE_ID);
    defer deleteEventStream(&redis, LC_ZOMBIE_ID);
    defer cleanupEventRows(db_ctx.conn, LC_ZOMBIE_ID);
    defer cleanupTelemetry(db_ctx.conn, LC_ZOMBIE_ID);

    try redis_zombie.ensureZombieConsumerGroup(&redis, LC_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .script_json = LIFECYCLE_SCRIPT });
    defer harness.deinit();

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = LC_ZOMBIE_ID,
        .workspace_id = LC_WORKSPACE_ID,
        .actor = TEST_ACTOR,
        .event_type = .chat,
        .request_json = TEST_REQUEST_JSON,
        .created_at = std.time.milliTimestamp(),
    };
    const xadded_id = try redis.xaddZombieEvent(envelope);
    defer ALLOC.free(xadded_id);

    var evt = (try redis_zombie.xreadgroupZombie(&redis, LC_ZOMBIE_ID, "lc-consumer")) orelse
        return error.UnexpectedNullEvent;
    defer evt.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, LC_ZOMBIE_ID, db_ctx.pool);
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

    try assertSingleProcessedRow(db_ctx.pool, LC_ZOMBIE_ID, evt.event_id);
    try assertReceiveAndStageTelemetryRows(db_ctx.pool, LC_ZOMBIE_ID, evt.event_id);
    try assertCheckpointedSession(db_ctx.pool, LC_ZOMBIE_ID);
}

test "integration: replaying the same event_id twice does not duplicate any of the three rows" {
    try skipIfHarnessDisabled();

    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = (try openRedis(ALLOC)) orelse return error.SkipZigTest;
    defer redis.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, RP_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, RP_WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, RP_WORKSPACE_ID);
    defer base.teardownPlatformProvider(db_ctx.conn, RP_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, RP_ZOMBIE_ID, RP_WORKSPACE_ID, helpers.ZOMBIE_NAME, helpers.ZOMBIE_CONFIG_JSON, helpers.ZOMBIE_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, RP_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, RP_SESSION_ID, RP_ZOMBIE_ID, EMPTY_CONTEXT_JSON);

    deleteEventStream(&redis, RP_ZOMBIE_ID);
    defer deleteEventStream(&redis, RP_ZOMBIE_ID);
    defer cleanupEventRows(db_ctx.conn, RP_ZOMBIE_ID);
    defer cleanupTelemetry(db_ctx.conn, RP_ZOMBIE_ID);

    try redis_zombie.ensureZombieConsumerGroup(&redis, RP_ZOMBIE_ID);

    var harness = try Harness.start(ALLOC, .{ .script_json = LIFECYCLE_SCRIPT });
    defer harness.deinit();

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = RP_ZOMBIE_ID,
        .workspace_id = RP_WORKSPACE_ID,
        .actor = TEST_ACTOR,
        .event_type = .chat,
        .request_json = TEST_REQUEST_JSON,
        .created_at = std.time.milliTimestamp(),
    };
    const xadded_id = try redis.xaddZombieEvent(envelope);
    defer ALLOC.free(xadded_id);

    var evt1 = (try redis_zombie.xreadgroupZombie(&redis, RP_ZOMBIE_ID, "rp-consumer-1")) orelse
        return error.UnexpectedNullEvent;
    defer evt1.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, RP_ZOMBIE_ID, db_ctx.pool);
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
    var consec1: u32 = 0;
    _ = writepath.run(ALLOC, &session, &evt1, cfg, &consec1);

    // Simulate XAUTOCLAIM redelivery: construct a synthetic ZombieEvent
    // carrying the identical event_id and run writepath again. The
    // worker's INSERT-on-conflict-do-nothing + telemetry UNIQUE
    // constraint must keep both tables at exactly 1 row.
    var evt2 = redis_zombie.ZombieEvent{
        .event_id = try ALLOC.dupe(u8, evt1.event_id),
        .workspace_id = try ALLOC.dupe(u8, evt1.workspace_id),
        .actor = try ALLOC.dupe(u8, evt1.actor),
        .event_type = try ALLOC.dupe(u8, evt1.event_type),
        .request_json = try ALLOC.dupe(u8, evt1.request_json),
        .created_at_ms = evt1.created_at_ms,
    };
    defer evt2.deinit(ALLOC);
    var consec2: u32 = 0;
    _ = writepath.run(ALLOC, &session, &evt2, cfg, &consec2);

    try assertSingleProcessedRow(db_ctx.pool, RP_ZOMBIE_ID, evt1.event_id);
    try assertReceiveAndStageTelemetryRows(db_ctx.pool, RP_ZOMBIE_ID, evt1.event_id);
    try assertCheckpointedSession(db_ctx.pool, RP_ZOMBIE_ID);
}

// ── helpers ────────────────────────────────────────────────────────────────

fn assertSingleProcessedRow(pool: *pg.Pool, zombie_id: []const u8, event_id: []const u8) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::int, max(status), max(response_text)
        \\FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ zombie_id, event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqual(@as(i32, 1), try row.get(i32, 0));
    try std.testing.expectEqualStrings(STATUS_PROCESSED, try row.get([]const u8, 1));
    try std.testing.expectEqualStrings("ok", try row.get([]const u8, 2));
}

fn assertReceiveAndStageTelemetryRows(pool: *pg.Pool, zombie_id: []const u8, event_id: []const u8) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    // Two-debit model: each event lands one `receive` row + one `stage` row.
    // UNIQUE (event_id, charge_type) keeps replays at exactly 2.
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::int,
        \\       count(*) FILTER (WHERE charge_type = 'receive')::int,
        \\       count(*) FILTER (WHERE charge_type = 'stage')::int
        \\FROM zombie_execution_telemetry
        \\WHERE zombie_id = $1 AND event_id = $2
    , .{ zombie_id, event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqual(@as(i32, 2), try row.get(i32, 0));
    try std.testing.expectEqual(@as(i32, 1), try row.get(i32, 1));
    try std.testing.expectEqual(@as(i32, 1), try row.get(i32, 2));
}

fn assertCheckpointedSession(pool: *pg.Pool, zombie_id: []const u8) !void {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT count(*)::int, max(checkpoint_at)
        \\FROM core.zombie_sessions
        \\WHERE zombie_id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqual(@as(i32, 1), try row.get(i32, 0));
    try std.testing.expect(try row.get(i64, 1) > 0);
}
