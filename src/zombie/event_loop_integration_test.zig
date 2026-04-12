// M1_001 §3.0 — Zombie event loop integration tests (DB-backed).
//
// Covers spec dimensions:
//   3.1 claimZombie — config + checkpoint loaded from DB
//   3.3 crashRecovery — agent reloaded from checkpoint
//
// Requires: LIVE_DB=1 TEST_DATABASE_URL=postgres://... (set by make test-integration-db).
// Skipped when DB is not available.

const std = @import("std");
const pg = @import("pg");
const event_loop = @import("event_loop.zig");
const zombie_config = @import("config.zig");
const base = @import("../db/test_fixtures.zig");

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-000000000099";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-000000000100";
const TEST_SESSION_ID = "0195b4ba-8d3a-7f13-8abc-000000000101";

const VALID_CONFIG_JSON =
    \\{"name":"lead-collector","trigger":{"type":"webhook","source":"agentmail"},"skills":["agentmail"],"budget":{"daily_dollars":5.0}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: lead-collector
    \\---
    \\
    \\You are a lead collector agent. Reply to every inbound email with a friendly greeting.
;

// ── 3.1: claimZombie — load config + session checkpoint ──────────────────

test "integration 3.1: claimZombie loads config from core.zombies" {
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

    try std.testing.expectEqualStrings("lead-collector", session.config.name);
    try std.testing.expectEqualStrings(TEST_ZOMBIE_ID, session.zombie_id);
    try std.testing.expectEqualStrings(TEST_WORKSPACE_ID, session.workspace_id);
    try std.testing.expectEqualStrings("agentmail", session.config.trigger.webhook.source);
}

test "integration 3.1: claimZombie extracts instructions from source_markdown" {
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

    // Instructions should be the markdown body after the frontmatter
    try std.testing.expect(session.instructions.len > 0);
    try std.testing.expect(std.mem.indexOf(u8, session.instructions, "lead collector agent") != null);
}

test "integration 3.1: claimZombie returns fresh session when no checkpoint exists" {
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

    // No checkpoint seeded → context_json should be "{}"
    try std.testing.expectEqualStrings("{}", session.context_json);
}

test "integration 3.1: claimZombie returns ZombieNotFound for unknown zombie_id" {
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try std.testing.expectError(
        error.ZombieNotFound,
        event_loop.claimZombie(alloc, "0195b4ba-8d3a-7f13-8abc-ffffffffffff", db_ctx.pool),
    );
}

test "integration 3.1: claimZombie returns ZombieNotActive for paused zombie" {
    const alloc = std.testing.allocator;
    const db_ctx = (try base.openTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);

    // Seed zombie, then update status to 'paused'
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "lead-collector", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    _ = try db_ctx.conn.exec(
        "UPDATE core.zombies SET status = 'paused' WHERE id = $1",
        .{TEST_ZOMBIE_ID},
    );

    try std.testing.expectError(
        error.ZombieNotActive,
        event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool),
    );
}

// ── 3.3: crashRecovery — agent reloaded from checkpoint ──────────────────

test "integration 3.3: claimZombie loads existing checkpoint from zombie_sessions" {
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

    // Seed a checkpoint with conversation context
    const checkpoint_json =
        \\{"last_event_id":"evt_prior","last_response":"I helped them yesterday."}
    ;
    try base.seedZombieSession(db_ctx.conn, TEST_SESSION_ID, TEST_ZOMBIE_ID, checkpoint_json);

    var session = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(alloc);

    // Checkpoint should be loaded — not the default "{}"
    try std.testing.expect(!std.mem.eql(u8, session.context_json, "{}"));
    try std.testing.expect(std.mem.indexOf(u8, session.context_json, "evt_prior") != null);
}

test "integration 3.3: checkpointState writes session to zombie_sessions" {
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

    // Claim with empty checkpoint, update context, then checkpoint
    var session = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(alloc);

    try event_loop.updateSessionContext(alloc, &session, "evt_new", "Replied to the lead.");
    try event_loop.checkpointState(alloc, &session, db_ctx.pool);

    // Re-claim the zombie — checkpoint should be loaded
    var session2 = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session2.deinit(alloc);

    try std.testing.expect(std.mem.indexOf(u8, session2.context_json, "evt_new") != null);
}

test "integration 3.3: checkpointState UPSERT overwrites previous checkpoint" {
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

    // First checkpoint
    try event_loop.updateSessionContext(alloc, &session, "evt_001", "first");
    try event_loop.checkpointState(alloc, &session, db_ctx.pool);

    // Second checkpoint (UPSERT should overwrite)
    try event_loop.updateSessionContext(alloc, &session, "evt_002", "second");
    try event_loop.checkpointState(alloc, &session, db_ctx.pool);

    // Re-claim — should see the SECOND checkpoint, not the first
    var session2 = try event_loop.claimZombie(alloc, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session2.deinit(alloc);

    try std.testing.expect(std.mem.indexOf(u8, session2.context_json, "evt_002") != null);
    // First checkpoint should be overwritten
    try std.testing.expect(std.mem.indexOf(u8, session2.context_json, "evt_001") == null);
}

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
