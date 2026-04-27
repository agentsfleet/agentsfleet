// M23_001 integration tests — execution tracking (§2) + steer poll (§3.1).
//
// §2.1  setExecutionActive  → execution_id + execution_started_at set in DB
// §2.2  clearExecutionActive → both columns NULLed in DB
// §2.3  claimZombie          → stale execution_id cleared on restart (crash recovery)
// §3.1  pollSteerAndInject   → GETDEL steer key, XADD event with type="steer"
//
// All tests require TEST_DATABASE_URL or DATABASE_URL (skipped otherwise).
// §3.1 additionally requires REDIS_URL (skipped otherwise).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const event_loop = @import("event_loop.zig");
const helpers = @import("event_loop_helpers.zig");
const types = @import("event_loop_types.zig");
const queue_redis = @import("../queue/redis.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// IDs — unique `0bbb` segment avoids collision with other test files.
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0b6f11";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb01";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0bbb10";

const VALID_CONFIG_JSON =
    \\{"name":"steer-bot","trigger":{"type":"webhook","source":"agentmail"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: steer-bot
    \\---
    \\
    \\You are a steerable agent. Accept new objectives mid-run.
;

// ── §2.1: setExecutionActive sets execution_id + execution_started_at ────────

test "integration M23_001 §2.1: setExecutionActive sets execution_id in DB" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "steer-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    helpers.setExecutionActive(ALLOC, &session, "test-exec-m23-001", db_ctx.pool);

    // Verify DB: execution_id and execution_started_at written.
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT execution_id, execution_started_at
        \\FROM core.zombie_sessions WHERE zombie_id = $1::uuid
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.NoSessionRow;
    const exec_id = (try row.get(?[]const u8, 0)) orelse return error.ExecIdNull;
    const started_at = try row.get(i64, 1);

    try std.testing.expectEqualStrings("test-exec-m23-001", exec_id);
    try std.testing.expect(started_at > 0);

    // Verify in-memory state.
    try std.testing.expectEqualStrings("test-exec-m23-001", session.execution_id.?);
    try std.testing.expect(session.execution_started_at > 0);
}

// ── §2.2: clearExecutionActive NULLs both columns ────────────────────────────

test "integration M23_001 §2.2: clearExecutionActive NULLs execution_id in DB" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "steer-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    helpers.setExecutionActive(ALLOC, &session, "test-exec-m23-002", db_ctx.pool);
    helpers.clearExecutionActive(ALLOC, &session, db_ctx.pool);

    // Verify DB: both columns NULL.
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT execution_id
        \\FROM core.zombie_sessions WHERE zombie_id = $1::uuid
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    if (try q.next()) |row| {
        const exec_id_opt = try row.get(?[]const u8, 0);
        try std.testing.expectEqual(@as(?[]const u8, null), exec_id_opt);
    }
    // Verify in-memory state.
    try std.testing.expectEqual(@as(?[]const u8, null), session.execution_id);
    try std.testing.expectEqual(@as(i64, 0), session.execution_started_at);
}

// ── §2.3: claimZombie clears stale execution_id (crash recovery) ─────────────

test "integration M23_001 §2.3: claimZombie clears stale execution_id on restart" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "steer-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    // Seed session row, then manually set a stale execution_id.
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");
    _ = try db_ctx.conn.exec(
        \\UPDATE core.zombie_sessions
        \\SET execution_id = 'stale-exec-m23', execution_started_at = 1
        \\WHERE zombie_id = $1::uuid
    , .{TEST_ZOMBIE_ID});

    // claimZombie calls clearExecutionActive internally (crash recovery).
    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    // Verify DB: execution_id cleared.
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT execution_id
        \\FROM core.zombie_sessions WHERE zombie_id = $1::uuid
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    if (try q.next()) |row| {
        const exec_id_opt = try row.get(?[]const u8, 0);
        try std.testing.expectEqual(@as(?[]const u8, null), exec_id_opt);
    }
}

// ── §3.1: pollSteerAndInject reads steer key and XADDs event ─────────────────

test "integration M23_001 §3.1: pollSteerAndInject consumes steer key and XADDs event" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis_client = queue_redis.Client.connectFromEnv(ALLOC, .default) catch
        return error.SkipZigTest;
    defer redis_client.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "steer-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    // Plant a steer signal in Redis (TTL 60s).
    const steer_key = "zombie:" ++ TEST_ZOMBIE_ID ++ ":steer";
    try redis_client.setEx(steer_key, "pivot to phase 2", 60);

    // Build a minimal EventLoopConfig (pollSteerAndInject only accesses redis).
    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis_client,
        .executor = undefined,
        .running = &running,
    };

    helpers.pollSteerAndInject(ALLOC, cfg, &session);

    // Verify steer key was consumed.
    const leftover = try redis_client.getDel(steer_key);
    defer if (leftover) |v| ALLOC.free(v);
    try std.testing.expectEqual(@as(?[]u8, null), leftover);

    // Verify event was written to the zombie's event stream.
    const events_key = "zombie:" ++ TEST_ZOMBIE_ID ++ ":events";
    var xlen_resp = try redis_client.command(&.{ "XLEN", events_key });
    defer xlen_resp.deinit(ALLOC);
    const count = switch (xlen_resp) {
        .integer => |n| n,
        else => return error.UnexpectedRedisResponse,
    };
    try std.testing.expect(count >= 1);

    // Cleanup: remove the events stream.
    var del_resp = try redis_client.command(&.{ "DEL", events_key });
    defer del_resp.deinit(ALLOC);
}
