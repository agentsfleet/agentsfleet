//! Live PostgreSQL and Redis proofs for repair-verification dispatch fencing.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");

const base = @import("../db/test_fixtures.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const dispatcher = @import("repair_verification_dispatcher.zig");
const queue_constants = @import("../queue/constants.zig");
const redis = @import("../queue/redis.zig");
const RedisPool = @import("../queue/redis_pool.zig");
const repair_queue = @import("../queue/redis_repair_verification.zig");
const repair_evidence = @import("../state/repair_evidence.zig");
const repair_verifications = @import("../state/repair_verifications.zig");

const testing = std.testing;
const ALLOC = testing.allocator;

const DISPATCHER_COUNT: usize = 100;
const REDIS_URL_ENV: [:0]const u8 = "TEST_REDIS_TLS_URL";
const REDIS_ACQUIRE_TIMEOUT_MS: u32 = 10_000;
const POLL_ATTEMPTS: usize = 1_000;
const POLL_INTERVAL_NS: u64 = 2 * std.time.ns_per_ms;

const NOW_MS: i64 = 1_800_000_000_000;
const TENANT_ID = "0195c102-7100-7000-8000-000000000001";
const WORKSPACE_ID = "0195c102-7101-7000-8000-000000000001";
const INCIDENT_FLEET_ID = "0195c102-7102-7000-8000-000000000001";
const REPAIR_LINK_ID = "0195c102-7103-7000-8000-000000000001";
const PRODUCTION_RESULT_ID = "0195c102-7104-7000-8000-000000000001";
const VERIFIER_FLEET_ONE = "0195c102-7201-7000-8000-000000000001";
const VERIFIER_FLEET_TWO = "0195c102-7202-7000-8000-000000000002";
const VERIFICATION_ONE = "0195c102-7301-7000-8000-000000000001";
const VERIFICATION_TWO = "0195c102-7302-7000-8000-000000000002";
const INCIDENT_EVENT_ID = "repair-dispatch-incident";
const RECOVERED_EVENT_ID = "repair-dispatch-recovered";
const REPOSITORY = "agentsfleet/agentsfleet";
const MERGED_COMMIT_SHA = "repair-dispatch-merged-commit";
const FLEET_STATUS_ACTIVE = "active";
const EVENT_STATUS_PROCESSED = "processed";
const REPAIR_STATUS_PENDING = "pending";
const FLEET_SOURCE = "# repair dispatcher integration fixture";
const FLEET_CONFIG = "{}";
const POISON_VALUE = "not-a-stream";

const VERIFIER_FLEETS = [_][]const u8{ VERIFIER_FLEET_ONE, VERIFIER_FLEET_TWO };
const VERIFICATION_IDS = [_][]const u8{ VERIFICATION_ONE, VERIFICATION_TWO };

fn resetFixture(conn: *pg.Conn) !void {
    _ = try conn.exec("BEGIN", .{});
    errdefer _ = conn.exec("ROLLBACK", .{}) catch null;
    _ = try conn.exec("SET LOCAL fleet.allow_gate_purge = 'on'", .{});
    _ = try conn.exec("DELETE FROM core.workspaces WHERE id = $1::uuid", .{WORKSPACE_ID});
    _ = try conn.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{TENANT_ID});
    _ = try conn.exec("COMMIT", .{});
}

fn resetFixtureBestEffort(conn: *pg.Conn) void {
    resetFixture(conn) catch |err| std.log.warn("repair dispatcher fixture cleanup ignored: {s}", .{@errorName(err)});
}

fn resetFixtureFromPool(pool: *pg.Pool) void {
    const conn = pool.acquire() catch |err| {
        std.log.warn("repair dispatcher cleanup acquire ignored: {s}", .{@errorName(err)});
        return;
    };
    defer pool.release(conn);
    resetFixtureBestEffort(conn);
}

fn seedFixture(conn: *pg.Conn, verifier_count: i32) !void {
    std.debug.assert(verifier_count > 0 and verifier_count <= VERIFIER_FLEETS.len);
    try base.seedTenantById(conn, TENANT_ID, "repair-dispatch-suite");
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json,
        \\   status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6::jsonb, $7, $8, $8)
    , .{
        INCIDENT_FLEET_ID,
        WORKSPACE_ID,
        TENANT_ID,
        "repair-dispatch-incident",
        FLEET_SOURCE,
        FLEET_CONFIG,
        FLEET_STATUS_ACTIVE,
        NOW_MS,
    });
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, tenant_id, name, source_markdown, config_json,
        \\   status, created_at, updated_at)
        \\SELECT
        \\  ('0195c102-72' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  $1::uuid, $2::uuid, 'repair-dispatch-verifier-' || g::text,
        \\  $3, $4::jsonb, $5, $6, $6
        \\FROM generate_series(1, $7::int) AS g
    , .{ WORKSPACE_ID, TENANT_ID, FLEET_SOURCE, FLEET_CONFIG, FLEET_STATUS_ACTIVE, NOW_MS, verifier_count });
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (fleet_id, workspace_id, event_id, actor, event_type, status,
        \\   request_json, response_text, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7::jsonb, $8, $9, $9)
    , .{
        INCIDENT_FLEET_ID,
        WORKSPACE_ID,
        INCIDENT_EVENT_ID,
        "test:incident",
        repair_verifications.WEBHOOK_TRIGGER,
        EVENT_STATUS_PROCESSED,
        "{\"symptom\":\"latency\"}",
        "Latency began after the deployment.",
        NOW_MS - 4,
    });
    _ = try conn.exec(
        \\INSERT INTO core.repair_pr_links
        \\  (id, workspace_id, fleet_id, event_id, repository, branch,
        \\   pr_number, pr_url, deploy_status, created_at,
        \\   merged_commit_sha, merged_at)
        \\VALUES
        \\  ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9, $10, $11, $12)
    , .{
        REPAIR_LINK_ID,
        WORKSPACE_ID,
        INCIDENT_FLEET_ID,
        INCIDENT_EVENT_ID,
        REPOSITORY,
        "agentsfleet-repair/repair-dispatch-incident",
        @as(i64, 157),
        "https://github.com/agentsfleet/agentsfleet/pull/157",
        REPAIR_STATUS_PENDING,
        NOW_MS - 3,
        MERGED_COMMIT_SHA,
        NOW_MS - 2,
    });
    _ = try conn.exec(
        \\INSERT INTO core.repair_production_results
        \\  (id, workspace_id, provider, provider_deployment_id,
        \\   provider_status_id, repository, environment, commit_sha,
        \\   conclusion, completed_at, created_at)
        \\VALUES
        \\  ($1::uuid, $2::uuid, $3, $4, $5, $6, $7, $8, $9, $10, $10)
    , .{
        PRODUCTION_RESULT_ID,
        WORKSPACE_ID,
        repair_evidence.GITHUB_PROVIDER,
        "repair-dispatch-deployment",
        "repair-dispatch-status",
        REPOSITORY,
        repair_evidence.PRODUCTION_ENVIRONMENT,
        MERGED_COMMIT_SHA,
        repair_evidence.SUCCESS_CONCLUSION,
        NOW_MS - 1,
    });
    _ = try conn.exec(
        \\INSERT INTO core.repair_verifications
        \\  (id, workspace_id, production_result_id, repair_link_id,
        \\   verifier_fleet_id, verify_after, dispatch_attempts,
        \\   created_at, updated_at)
        \\SELECT
        \\  ('0195c102-73' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  $1::uuid, $2::uuid, $3::uuid,
        \\  ('0195c102-72' || lpad(to_hex(g), 2, '0') ||
        \\   '-7000-8000-' || lpad(to_hex(g), 12, '0'))::uuid,
        \\  $4, 0, $4, $4
        \\FROM generate_series(1, $5::int) AS g
    , .{ WORKSPACE_ID, PRODUCTION_RESULT_ID, REPAIR_LINK_ID, NOW_MS, verifier_count });
}

fn redisOrSkip(alloc: std.mem.Allocator) !redis.Client {
    const url = common.env.testLiveValue(REDIS_URL_ENV) orelse return error.SkipZigTest;
    return redis.testing.connectFromUrl(common.globalIo(), alloc, url);
}

fn boundedRedisOrSkip(alloc: std.mem.Allocator) !redis.Client {
    const url = common.env.testLiveValue(REDIS_URL_ENV) orelse return error.SkipZigTest;
    const cfg = try redis.testing.poolConfigFromUrl(alloc, url);
    var pool = try RedisPool.init(common.globalIo(), alloc, cfg, .{
        .max_idle = 1,
        .eager_min = 1,
        .max_active = 1,
        .acquire_timeout_ms = REDIS_ACQUIRE_TIMEOUT_MS,
    });
    errdefer pool.deinit();
    return .{ .alloc = alloc, .pool = pool };
}

fn cleanupRedis(client: *redis.Client) !void {
    for (VERIFICATION_IDS) |verification_id| try repair_queue.clearOnce(client, verification_id);
    for (VERIFIER_FLEETS) |fleet_id| {
        var key_buf: [queue_constants.fleet_stream_key_buf_len]u8 = undefined;
        const stream_key = try queue_constants.fleetStreamKey(&key_buf, fleet_id);
        try client.del(stream_key);
        var response = try client.command(&.{ "HDEL", queue_constants.ready_index_key, fleet_id });
        response.deinit(client.alloc);
    }
}

fn cleanupRedisBestEffort(client: *redis.Client) void {
    cleanupRedis(client) catch |err| std.log.warn("repair dispatcher Redis cleanup ignored: {s}", .{@errorName(err)});
}

fn poisonStream(client: *redis.Client, fleet_id: []const u8) !void {
    var key_buf: [queue_constants.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_constants.fleetStreamKey(&key_buf, fleet_id);
    var response = try client.command(&.{ "SET", stream_key, POISON_VALUE });
    defer response.deinit(client.alloc);
}

fn deleteStream(client: *redis.Client, fleet_id: []const u8) !void {
    var key_buf: [queue_constants.fleet_stream_key_buf_len]u8 = undefined;
    try client.del(try queue_constants.fleetStreamKey(&key_buf, fleet_id));
}

fn streamLength(client: *redis.Client, fleet_id: []const u8) !i64 {
    var key_buf: [queue_constants.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_constants.fleetStreamKey(&key_buf, fleet_id);
    var response = try client.command(&.{ "XLEN", stream_key });
    defer response.deinit(client.alloc);
    return switch (response) {
        .integer => |count| count,
        else => error.TestUnexpectedResult,
    };
}

fn expectVerificationState(
    conn: *pg.Conn,
    verification_id: []const u8,
    attempts: i64,
    claimed: bool,
    has_event: bool,
) !void {
    var query = PgQuery.from(try conn.query(
        \\SELECT dispatch_attempts, dispatch_claim_token IS NOT NULL,
        \\       verifier_event_id
        \\FROM core.repair_verifications
        \\WHERE id = $1::uuid
    , .{verification_id}));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    try testing.expectEqual(attempts, try row.get(i64, 0));
    try testing.expectEqual(claimed, try row.get(bool, 1));
    try testing.expectEqual(has_event, (try row.get(?[]const u8, 2)) != null);
}

fn expectEvent(conn: *pg.Conn, verification_id: []const u8, event_id: []const u8) !void {
    var query = PgQuery.from(try conn.query(
        "SELECT verifier_event_id FROM core.repair_verifications WHERE id = $1::uuid",
        .{verification_id},
    ));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings(event_id, try row.get(?[]const u8, 0) orelse return error.TestUnexpectedResult);
}

fn expectRedisCleanupCount(conn: *pg.Conn, expected: i64) !void {
    var query = PgQuery.from(try conn.query(
        "SELECT count(*) FROM core.repair_verifications WHERE redis_once_key_cleared_at IS NOT NULL",
        .{},
    ));
    defer query.deinit();
    const row = try query.next() orelse return error.TestUnexpectedResult;
    try testing.expectEqual(expected, try row.get(i64, 0));
}

fn claimPresent(pool: *pg.Pool) !bool {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var query = PgQuery.from(try conn.query(
        "SELECT dispatch_claim_token IS NOT NULL FROM core.repair_verifications WHERE id = $1::uuid",
        .{VERIFICATION_ONE},
    ));
    defer query.deinit();
    const row = try query.next() orelse return false;
    return row.get(bool, 0);
}

fn waitForClaim(pool: *pg.Pool) !void {
    for (0..POLL_ATTEMPTS) |_| {
        if (try claimPresent(pool)) return;
        common.sleepNanos(POLL_INTERVAL_NS);
    }
    return error.ClaimNotObserved;
}

fn waitForDatabaseIdle(pool: *pg.Pool) !void {
    for (0..POLL_ATTEMPTS) |_| {
        if (pool.stats().in_use == 0) return;
        common.sleepNanos(POLL_INTERVAL_NS);
    }
    return error.DatabaseConnectionStillHeld;
}

const ConcurrentDispatchWorker = struct {
    pool: *pg.Pool,
    queue: *redis.Client,
    ready: *std.atomic.Value(usize),
    gate: *std.atomic.Value(bool),
    stats: dispatcher.Stats = .{},
    err: ?anyerror = null,

    fn run(self: *ConcurrentDispatchWorker) void {
        _ = self.ready.fetchAdd(1, .acq_rel);
        while (!self.gate.load(.acquire)) std.atomic.spinLoopHint();
        self.stats = dispatcher.dispatchOnce(self.pool, self.queue, ALLOC, NOW_MS) catch |err| {
            self.err = err;
            return;
        };
    }
};

const BlockingDispatchWorker = struct {
    pool: *pg.Pool,
    queue: *redis.Client,
    stats: dispatcher.Stats = .{},
    err: ?anyerror = null,
    done: std.atomic.Value(bool) = .init(false),

    fn run(self: *BlockingDispatchWorker) void {
        defer self.done.store(true, .release);
        self.stats = dispatcher.dispatchOnce(self.pool, self.queue, ALLOC, NOW_MS) catch |err| {
            self.err = err;
            return;
        };
    }
};

test "integration: one intent is claimed once by one hundred concurrent dispatchers" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn, 1);

    var queue = try redisOrSkip(ALLOC);
    defer queue.deinit();
    try cleanupRedis(&queue);
    defer cleanupRedisBestEffort(&queue);

    var workers: [DISPATCHER_COUNT]ConcurrentDispatchWorker = undefined;
    var threads: [DISPATCHER_COUNT]std.Thread = undefined;
    var ready = std.atomic.Value(usize).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var spawned: usize = 0;
    var joined = false;
    defer {
        gate.store(true, .release);
        if (!joined) for (threads[0..spawned]) |*thread| thread.join();
    }
    for (&workers, &threads) |*worker, *thread| {
        worker.* = .{ .pool = db.pool, .queue = &queue, .ready = &ready, .gate = &gate };
        thread.* = try std.Thread.spawn(.{}, ConcurrentDispatchWorker.run, .{worker});
        spawned += 1;
    }
    while (ready.load(.acquire) != DISPATCHER_COUNT) std.atomic.spinLoopHint();
    gate.store(true, .release);
    for (&threads) |*thread| thread.join();
    joined = true;

    var due: usize = 0;
    var completed: usize = 0;
    for (&workers) |*worker| {
        try testing.expect(worker.err == null);
        due += worker.stats.due;
        completed += worker.stats.completed;
    }
    try testing.expectEqual(@as(usize, 1), due);
    try testing.expectEqual(@as(usize, 1), completed);
    try expectVerificationState(db.conn, VERIFICATION_ONE, 1, false, true);
    try testing.expectEqual(@as(i64, 1), try streamLength(&queue, VERIFIER_FLEET_ONE));
    try testing.expectEqual(@as(usize, 1), db.pool.stats().in_use);
}

test "integration: stale repair verification claim recovers through its token fence" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn, 1);

    var first = try repair_verifications.claimDue(ALLOC, db.conn, NOW_MS);
    defer first.deinit(ALLOC);
    try testing.expectEqual(@as(usize, 1), first.items.len);

    var still_owned = try repair_verifications.claimDue(
        ALLOC,
        db.conn,
        NOW_MS + repair_verifications.CLAIM_STALE_MS - 1,
    );
    defer still_owned.deinit(ALLOC);
    try testing.expectEqual(@as(usize, 0), still_owned.items.len);

    var recovered = try repair_verifications.claimDue(
        ALLOC,
        db.conn,
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    );
    defer recovered.deinit(ALLOC);
    try testing.expectEqual(@as(usize, 1), recovered.items.len);
    try testing.expect(!std.mem.eql(u8, first.token, recovered.token));
    try testing.expect(!try repair_verifications.complete(
        db.conn,
        VERIFICATION_ONE,
        first.token,
        "stale-owner-must-lose",
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    ));
    try testing.expect(try repair_verifications.complete(
        db.conn,
        VERIFICATION_ONE,
        recovered.token,
        RECOVERED_EVENT_ID,
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    ));
    try expectVerificationState(db.conn, VERIFICATION_ONE, 2, false, true);
    try expectEvent(db.conn, VERIFICATION_ONE, RECOVERED_EVENT_ID);
}

test "integration: completed once keys are marked by one set-based update" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn, 2);

    var queue = try redisOrSkip(ALLOC);
    defer queue.deinit();
    try cleanupRedis(&queue);
    defer cleanupRedisBestEffort(&queue);

    var claimed = try repair_verifications.claimDue(ALLOC, db.conn, NOW_MS);
    defer claimed.deinit(ALLOC);
    try testing.expectEqual(@as(usize, 2), claimed.items.len);
    for (claimed.items, 0..) |item, index| {
        try testing.expect(try repair_verifications.complete(
            db.conn,
            item.id,
            claimed.token,
            if (index == 0) "cleanup-event-one" else "cleanup-event-two",
            NOW_MS,
        ));
    }
    try expectRedisCleanupCount(db.conn, 0);

    const stats = try dispatcher.dispatchOnce(
        db.pool,
        &queue,
        ALLOC,
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    );
    try testing.expectEqual(@as(usize, 0), stats.due);
    try testing.expect(!stats.cleanup_pending);
    try expectRedisCleanupCount(db.conn, 2);
}

test "integration: a live repair verification claim cannot be cleared without completion" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn, 1);

    var claimed = try repair_verifications.claimDue(ALLOC, db.conn, NOW_MS);
    defer claimed.deinit(ALLOC);
    try testing.expectEqual(@as(usize, 1), claimed.items.len);
    try testing.expectError(error.PG, db.conn.exec(
        \\UPDATE core.repair_verifications
        \\SET dispatch_claim_token = NULL, dispatch_claimed_at = NULL,
        \\    updated_at = $2
        \\WHERE id = $1::uuid
    , .{ VERIFICATION_ONE, NOW_MS + 1 }));
    try expectVerificationState(db.conn, VERIFICATION_ONE, 1, true, false);
}

test "integration: poison repair verification row does not starve later work" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    defer db.pool.release(db.conn);
    try resetFixture(db.conn);
    defer resetFixtureBestEffort(db.conn);
    try seedFixture(db.conn, 2);

    var queue = try redisOrSkip(ALLOC);
    defer queue.deinit();
    try cleanupRedis(&queue);
    defer cleanupRedisBestEffort(&queue);
    try poisonStream(&queue, VERIFIER_FLEET_ONE);

    const first = try dispatcher.dispatchOnce(db.pool, &queue, ALLOC, NOW_MS);
    try testing.expectEqual(@as(usize, 2), first.due);
    try testing.expectEqual(@as(usize, 1), first.completed);
    try testing.expectEqual(@as(usize, 1), first.failed);
    try expectVerificationState(db.conn, VERIFICATION_ONE, 1, true, false);
    try expectVerificationState(db.conn, VERIFICATION_TWO, 1, false, true);
    try testing.expectEqual(@as(i64, 1), try streamLength(&queue, VERIFIER_FLEET_TWO));

    try deleteStream(&queue, VERIFIER_FLEET_ONE);
    const fenced = try dispatcher.dispatchOnce(db.pool, &queue, ALLOC, NOW_MS + 1);
    try testing.expectEqual(@as(usize, 0), fenced.due);
    try testing.expectEqual(@as(usize, 0), fenced.completed);
    try expectVerificationState(db.conn, VERIFICATION_ONE, 1, true, false);

    const retry = try dispatcher.dispatchOnce(
        db.pool,
        &queue,
        ALLOC,
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    );
    try testing.expectEqual(@as(usize, 1), retry.due);
    try testing.expectEqual(@as(usize, 1), retry.completed);
    try expectVerificationState(db.conn, VERIFICATION_ONE, 2, false, true);
    try expectVerificationState(db.conn, VERIFICATION_TWO, 1, false, true);
    try testing.expectEqual(@as(i64, 1), try streamLength(&queue, VERIFIER_FLEET_ONE));
    try testing.expectEqual(@as(i64, 1), try streamLength(&queue, VERIFIER_FLEET_TWO));
}

test "integration: Redis failure waits with no database connection held and retries through the fence" {
    const db = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db.pool.deinit();
    var fixture_conn_held = true;
    defer if (fixture_conn_held) db.pool.release(db.conn);
    try resetFixture(db.conn);
    try seedFixture(db.conn, 1);
    defer resetFixtureFromPool(db.pool);
    db.pool.release(db.conn);
    fixture_conn_held = false;

    var control = try redisOrSkip(ALLOC);
    defer control.deinit();
    try cleanupRedis(&control);
    defer cleanupRedisBestEffort(&control);
    try poisonStream(&control, VERIFIER_FLEET_ONE);

    var queue = try boundedRedisOrSkip(ALLOC);
    defer queue.deinit();
    const held_redis = try queue.pool.acquire();
    var redis_slot_held = true;
    var worker: BlockingDispatchWorker = .{ .pool = db.pool, .queue = &queue };
    const thread = try std.Thread.spawn(.{}, BlockingDispatchWorker.run, .{&worker});
    var joined = false;
    defer {
        if (redis_slot_held) queue.pool.release(held_redis, true);
        if (!joined) thread.join();
    }

    try waitForClaim(db.pool);
    try waitForDatabaseIdle(db.pool);
    try testing.expect(!worker.done.load(.acquire));
    try testing.expectEqual(@as(usize, 0), db.pool.stats().in_use);
    try testing.expectEqual(@as(usize, 1), queue.pool.stats().active);

    queue.pool.release(held_redis, true);
    redis_slot_held = false;
    thread.join();
    joined = true;
    try testing.expect(worker.err == null);
    try testing.expectEqual(@as(usize, 1), worker.stats.due);
    try testing.expectEqual(@as(usize, 0), worker.stats.completed);
    try testing.expectEqual(@as(usize, 1), worker.stats.failed);

    {
        const conn = try db.pool.acquire();
        defer db.pool.release(conn);
        try expectVerificationState(conn, VERIFICATION_ONE, 1, true, false);
    }
    try deleteStream(&control, VERIFIER_FLEET_ONE);
    const fenced = try dispatcher.dispatchOnce(db.pool, &queue, ALLOC, NOW_MS + 1);
    try testing.expectEqual(@as(usize, 0), fenced.due);
    try testing.expectEqual(@as(usize, 0), fenced.completed);
    const retry = try dispatcher.dispatchOnce(
        db.pool,
        &queue,
        ALLOC,
        NOW_MS + repair_verifications.CLAIM_STALE_MS,
    );
    try testing.expectEqual(@as(usize, 1), retry.due);
    try testing.expectEqual(@as(usize, 1), retry.completed);
    {
        const conn = try db.pool.acquire();
        defer db.pool.release(conn);
        try expectVerificationState(conn, VERIFICATION_ONE, 2, false, true);
    }
    try testing.expectEqual(@as(usize, 0), db.pool.stats().in_use);
}
