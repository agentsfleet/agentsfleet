// Integration tests for the durable system-event row that reloadZombieConfig
// writes after a successful config swap.
//
// The reload happens inline in event_loop.reloadZombieConfig (which is pub
// for this exact test-seam purpose) — no worker thread or control-stream
// signal needed to exercise the emission path. The activity-channel publish
// is intentionally NOT part of this feature: state-change events live in the
// durable trace only and surface on the next /events poll.
//
// Requires LIVE_DB=1. Skipped when DB is not available.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const event_loop = @import("event_loop.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d0001";
const TEST_ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d0101";

const VALID_CONFIG_JSON =
    \\{"name":"reload-bot","x-usezombie":{"triggers":[{"type":"webhook","source":"github","events":["workflow_run"]}],"tools":["http_request"],"budget":{"daily_dollars":5.0}}}
;
const VALID_SOURCE_MD =
    \\---
    \\name: reload-bot
    \\---
    \\
    \\You are a reload-emit test agent.
;

fn deleteZombieEventsRows(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{TEST_ZOMBIE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn countSystemConfigRows(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*) FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND actor = 'system:config_update'
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    return try row.get(i64, 0);
}

test "integration: reloadZombieConfig writes system row with cfg-<revision> event_id" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "reload-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    defer deleteZombieEventsRows(db_ctx.conn);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);

    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT event_id, actor, event_type, status, response_text, request_json::text
        \\FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND actor = 'system:config_update'
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;

    const event_id = try row.get([]const u8, 0);
    try std.testing.expect(std.mem.startsWith(u8, event_id, "cfg-"));

    try std.testing.expectEqualStrings("system:config_update", try row.get([]const u8, 1));
    try std.testing.expectEqualStrings("system", try row.get([]const u8, 2));
    try std.testing.expectEqualStrings("processed", try row.get([]const u8, 3));

    const summary = (try row.get(?[]const u8, 4)) orelse return error.SummaryMissing;
    try std.testing.expect(std.mem.indexOf(u8, summary, "Configuration reloaded to revision") != null);

    const request_json = try row.get([]const u8, 5);
    try std.testing.expect(std.mem.indexOf(u8, request_json, "\"revision\"") != null);
}

test "integration: reloadZombieConfig is idempotent — same revision = no duplicate row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "reload-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    defer deleteZombieEventsRows(db_ctx.conn);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);
    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);
    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);

    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    try std.testing.expectEqual(@as(i64, 1), try countSystemConfigRows(conn));
}

test "integration: reloadZombieConfig writes a new row when revision changes" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "reload-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);
    defer deleteZombieEventsRows(db_ctx.conn);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);

    // Bump updated_at (simulates a fresh PATCH advancing the revision).
    const conn = try db_ctx.pool.acquire();
    defer db_ctx.pool.release(conn);
    _ = try conn.exec(
        "UPDATE core.zombies SET updated_at = updated_at + 1 WHERE id = $1::uuid",
        .{TEST_ZOMBIE_ID},
    );

    try event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool);

    try std.testing.expectEqual(@as(i64, 2), try countSystemConfigRows(conn));

    // The two event_ids must be distinct cfg-<revision> strings.
    var q = PgQuery.from(try conn.query(
        \\SELECT event_id FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND actor = 'system:config_update'
        \\ORDER BY event_id ASC
    , .{TEST_ZOMBIE_ID}));
    defer q.deinit();
    const a = (try q.next()) orelse return error.RowNotFound;
    const first = try ALLOC.dupe(u8, try a.get([]const u8, 0));
    defer ALLOC.free(first);
    const b = (try q.next()) orelse return error.RowNotFound;
    const second = try b.get([]const u8, 0);
    try std.testing.expect(!std.mem.eql(u8, first, second));
    try std.testing.expect(std.mem.startsWith(u8, first, "cfg-"));
    try std.testing.expect(std.mem.startsWith(u8, second, "cfg-"));
}

test "integration: reloadZombieConfig returns ZombieNotFound for deleted zombie" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try base.seedTenant(db_ctx.conn);
    defer base.teardownTenant(db_ctx.conn);
    try base.seedWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    defer base.teardownWorkspace(db_ctx.conn, TEST_WORKSPACE_ID);
    try base.seedZombie(db_ctx.conn, TEST_ZOMBIE_ID, TEST_WORKSPACE_ID, "reload-bot", VALID_CONFIG_JSON, VALID_SOURCE_MD);
    defer base.teardownZombies(db_ctx.conn, TEST_WORKSPACE_ID);

    var session = try event_loop.claimZombie(ALLOC, TEST_ZOMBIE_ID, db_ctx.pool);
    defer session.deinit(ALLOC);

    _ = try db_ctx.conn.exec("DELETE FROM core.zombies WHERE id = $1::uuid", .{TEST_ZOMBIE_ID});

    try std.testing.expectError(
        error.ZombieNotFound,
        event_loop.reloadZombieConfig(ALLOC, &session, db_ctx.pool),
    );
}
