// Integration tests for event_loop_writepath.run — the 13-step processEvent body.
//
// Slice 4c lands the gate-blocked happy path coverage that does not require an
// executor harness. Approval-gate / executor-streaming end-to-end tests live
// in slice 11 once the executor harness lands.
//
// Requires LIVE_DB=1 + a reachable Redis. Skipped when either is missing.

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
const balance_policy_mod = @import("../config/balance_policy.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f11";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa01";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa10";

const VALID_CONFIG_JSON =
    \\{"name":"writepath-bot","trigger":{"type":"webhook","source":"agentmail"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: writepath-bot
    \\---
    \\
    \\You are a write-path test agent.
;

fn deleteEventStream(redis: *queue_redis.Client) void {
    var resp = redis.command(&.{ "DEL", "zombie:" ++ TEST_ZOMBIE_ID ++ ":events" }) catch return;
    defer resp.deinit(redis.alloc);
}

fn deleteZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch {};
}

fn setBalanceExhausted(conn: *pg.Conn, exhausted_at_ms: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing
        \\  (tenant_id, plan_tier, plan_sku, balance_cents, grant_source,
        \\   balance_exhausted_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'free', 'free.v1', 0, 'test', $2, 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_exhausted_at = EXCLUDED.balance_exhausted_at,
        \\      balance_cents = EXCLUDED.balance_cents
    , .{ base.TEST_TENANT_ID, exhausted_at_ms });
}

fn clearTenantBilling(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM billing.tenant_billing WHERE tenant_id = $1::uuid", .{base.TEST_TENANT_ID}) catch {};
}

// ── Balance-exhausted gate: writepath must mark gate_blocked + XACK ─────────

test "integration: writepath.run on balance-exhausted tenant writes gate_blocked row + xacks" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var redis = queue_redis.connectFromEnv(ALLOC, .default) catch return error.SkipZigTest;
    defer redis.deinit();

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "writepath-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, "{}");

    try setBalanceExhausted(db_ctx.conn, std.time.milliTimestamp());
    defer clearTenantBilling(db_ctx.conn);

    deleteEventStream(&redis);
    defer deleteEventStream(&redis);
    defer deleteZombieEventsRows(db_ctx.conn);

    try redis_zombie.ensureZombieConsumerGroup(&redis, TEST_ZOMBIE_ID);

    // XADD an envelope onto the stream so XREADGROUP returns a real entry id.
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

    var evt = (try redis_zombie.xreadgroupZombie(&redis, TEST_ZOMBIE_ID, "writepath-test-consumer")) orelse
        return error.UnexpectedNullEvent;
    defer evt.deinit(ALLOC);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = &redis,
        .redis_publish = &redis,
        .executor = undefined, // gate fires before executor work
        .running = &running,
        .balance_policy = .stop,
    };
    var consec: u32 = 0;
    const next_consec = writepath.run(ALLOC, &session, &evt, cfg, &consec);
    try std.testing.expectEqual(@as(u32, 0), next_consec);

    // Verify zombie_events row: status=gate_blocked, failure_label=balance_exhausted,
    // response_text NULL, updated_at set.
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT status, failure_label, response_text, updated_at
        \\FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ TEST_ZOMBIE_ID, evt.event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("gate_blocked", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings("balance_exhausted", try row.get([]const u8, 1));
    try std.testing.expectEqual(@as(?[]const u8, null), try row.get(?[]const u8, 2));
    try std.testing.expect(try row.get(i64, 3) > 0);

    // Verify the XACK landed: XREADGROUP > on the same stream returns nothing
    // (the only entry was acknowledged).
    const second = try redis_zombie.xreadgroupZombie(&redis, TEST_ZOMBIE_ID, "writepath-test-consumer");
    if (second) |s| {
        var s2 = s;
        s2.deinit(ALLOC);
        return error.EventNotXAckedAfterGateBlock;
    }
}
