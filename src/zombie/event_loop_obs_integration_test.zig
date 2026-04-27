// M15_002 observability wiring — DB-backed integration tests.
// Extracted from event_loop_integration_test.zig to keep both under the
// 350-line gate.
//
// Requires: LIVE_DB=1 TEST_DATABASE_URL=postgres://... (set by make test-integration-db).
// Skipped when DB is not available.

const std = @import("std");
const pg = @import("pg");
const event_loop = @import("event_loop.zig");
const base = @import("../db/test_fixtures.zig");

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-000000000099";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-000000000100";

const VALID_CONFIG_JSON =
    \\{"name":"lead-collector","trigger":{"type":"webhook","source":"agentmail"},"tools":["agentmail"],"budget":{"daily_dollars":5.0}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: lead-collector
    \\---
    \\
    \\You are a lead collector agent. Reply to every inbound email with a friendly greeting.
;

// ── M15_002: observability wiring — metrics + PostHog capture ───────────────

test "integration M15_002: recordDeliverError increments failed counter + PostHog" {
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "lead-collector", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    var session = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(alloc);

    const metrics_zombie = @import("../observability/metrics_zombie.zig");
    const telemetry_mod = @import("../observability/telemetry.zig");
    const helpers = @import("event_loop_helpers.zig");
    const types = @import("event_loop_types.zig");

    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();
    var tel = telemetry_mod.Telemetry.initTest();

    // Build a running flag so EventLoopConfig is valid (recordDeliverError doesn't read it).
    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = undefined,
        .executor = undefined,
        .running = &running,
        .telemetry = &tel,
    };

    helpers.recordDeliverError(cfg, &session, "evt_abc");

    const s = metrics_zombie.snapshotZombieFields();
    try std.testing.expectEqual(@as(u64, 1), s.zombie_failed_total);
    try telemetry_mod.TestBackend.assertLastEventIs(.zombie_completed);

    metrics_zombie.resetForTest();
}

// Spec §6.0 row 3.2 — event_loop_emits_completion: delivery success updates all 4 counters.
test "integration M15_002 3.2: event_loop_emits_completion updates all 4 counters" {
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "lead-collector", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    var session = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(alloc);

    const metrics_zombie = @import("../observability/metrics_zombie.zig");
    const telemetry_mod = @import("../observability/telemetry.zig");
    const helpers = @import("event_loop_helpers.zig");
    const types = @import("event_loop_types.zig");
    const redis_zombie = @import("../queue/redis_zombie.zig");

    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();
    var tel = telemetry_mod.Telemetry.initTest();

    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = undefined,
        .executor = undefined,
        .running = &running,
        .telemetry = &tel,
    };

    var event = redis_zombie.ZombieEvent{
        .message_id = try alloc.dupe(u8, "0-1"),
        .event_id = try alloc.dupe(u8, "evt_success"),
        .event_type = try alloc.dupe(u8, "email.received"),
        .source = try alloc.dupe(u8, "webhook"),
        .data_json = try alloc.dupe(u8, "{}"),
    };
    defer event.deinit(alloc);
    // Shape-compatible stage_result stand-in (logDeliveryResult takes anytype).
    const Failure = struct {
        pub fn label(_: @This()) []const u8 {
            return "";
        }
    };
    const stage_result = struct {
        failure: ?Failure = null,
        token_count: u64 = 1500,
        wall_seconds: u64 = 2,
        time_to_first_token_ms: u64 = 0,
    }{};

    helpers.logDeliveryResult(cfg, alloc, &session, &event, stage_result, 4_200);

    const s = metrics_zombie.snapshotZombieFields();
    try std.testing.expectEqual(@as(u64, 1), s.zombie_completed_total);
    try std.testing.expectEqual(@as(u64, 1500), s.zombie_tokens_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 4_200), s.zombie_execution_seconds.sum);
    try std.testing.expectEqual(@as(u64, 0), s.zombie_failed_total);
    try telemetry_mod.TestBackend.assertLastEventIs(.zombie_completed);

    metrics_zombie.resetForTest();
}

// T2/T3 — null telemetry on recordDeliverError does not panic (prod fallback path).
test "integration M15_002: null telemetry in recordDeliverError does not panic" {
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "lead-collector", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    var session = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(alloc);

    const metrics_zombie = @import("../observability/metrics_zombie.zig");
    const helpers = @import("event_loop_helpers.zig");
    const types = @import("event_loop_types.zig");

    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();
    var running = std.atomic.Value(bool).init(true);
    const cfg = types.EventLoopConfig{
        .pool = db_ctx.pool,
        .redis = undefined,
        .executor = undefined,
        .running = &running,
        .telemetry = null, // explicit — prod fallback
    };

    helpers.recordDeliverError(cfg, &session, "evt_null_tel");

    // Metrics still record even without telemetry.
    try std.testing.expectEqual(
        @as(u64, 1),
        metrics_zombie.snapshotZombieFields().zombie_failed_total,
    );
    metrics_zombie.resetForTest();
}
