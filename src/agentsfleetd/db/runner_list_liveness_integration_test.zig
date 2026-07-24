//! Integration tier for the runner-list liveness read (schema slot 033 / the §3
//! restructure). Separate file from the index-fitness suite because it tests a
//! different thing: not whether an index fits a query, but that the lease-
//! liveness EXISTS is evaluated over the paginated result rather than by hashing
//! the whole lease table. That is a property of how the statement is written,
//! and the planner only reveals it once there is enough data for the two shapes
//! to cost differently -- so this is the one liveness assertion that stays sized
//! for the planner rather than pinned with `enable_seqscan = off`.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

const WS_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0001";
const FLEET_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0002";
const RUNNER_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0003";
const NAME_PREFIX = "idxprobe-live-";

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.conn);
        self.pool.deinit();
    }
};

fn planOf(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8) ![]u8 {
    const explain = try std.fmt.allocPrint(alloc, "EXPLAIN (COSTS OFF) {s}", .{sql});
    defer alloc.free(explain);
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var q = PgQuery.from(try conn.query(explain, .{}));
    defer q.deinit();
    while (try q.next()) |row| {
        try out.appendSlice(alloc, try row.get([]const u8, 0));
        try out.append(alloc, '\n');
    }
    return out.toOwnedSlice(alloc);
}

fn teardown(conn: *pg.Conn) void {
    // teardownFleets cascades runner_leases (FK ON DELETE CASCADE); the bulk
    // runners are host-prefixed and wiped explicitly.
    base.teardownFleets(conn, WS_PROBE);
    base.teardownWorkspace(conn, WS_PROBE);
    _ = conn.exec("DELETE FROM fleet.runners WHERE host_id LIKE $1", .{NAME_PREFIX ++ "%"}) catch |err|
        std.log.warn("runner wipe ignored: {s}", .{@errorName(err)});
}

fn seedProbeFleet(conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS_PROBE);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $2, 'standard', 'active', '[]'::jsonb, 0, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ RUNNER_PROBE, NAME_PREFIX ++ "runner" });
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'live-fleet', '', '{}'::jsonb, 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_PROBE, WS_PROBE });
}

/// Runners each holding their own live lease. This is the ONE fixture in this
/// file still sized for the planner, and deliberately so: the assertion below is
/// about our query's shape under real volume, not about an index's existence.
/// The crossover where the planner switches from hashing the whole lease table
/// to per-page probes is fuzzy and sits a few thousand rows up: 3 000 landed
/// on the knife edge and flipped between runs; 5 000 chose the per-page probe
/// on every repeat when measured. This is the one fixture in the index suites
/// that legitimately needs scale — the property under test IS a planner choice
/// that only appears once hashing the whole lease table costs more than 25
/// index probes.
const LIVENESS_RUNNERS: i32 = 5_000;

fn seedRunnersWithLeases(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\SELECT overlay(md5('lr' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, $1 || g, 'standard', 'active', '[]'::jsonb, 0, 1750000000000 + g, 0
        \\FROM generate_series(1, $2::int) g
        \\ON CONFLICT DO NOTHING
    , .{ NAME_PREFIX, LIVENESS_RUNNERS });
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\SELECT overlay(md5('LL' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       overlay(md5('lr' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2::uuid, $3::uuid, 'le' || g, 'a', 'fleet.run', '{}', 0,
        \\       'standard', 'anthropic', 'claude', 0, 0, 0, 0, g, 9999999999999,
        \\       'active', 0, 0
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_PROBE, WS_PROBE, base.TEST_TENANT_ID, LIVENESS_RUNNERS });
    _ = try conn.exec("ANALYZE fleet.runners", .{});
    _ = try conn.exec("ANALYZE fleet.runner_leases", .{});
}

test "runner list liveness is bounded by page size" {
    // The exception to this file's cheap-assertion rule, and worth stating why.
    // Everything above asks "does this index fit this query", which is size
    // independent. This asks something else: that the lease-liveness EXISTS is
    // evaluated over the PAGE rather than by hashing the whole lease table —
    // a property of how our statement is written, and one the planner only
    // reveals once there is enough data for the two shapes to cost differently.
    // Disabling seqscan would defeat the assertion outright, so this fixture
    // stays sized. It is the largest measured win in the change it guards
    // (6 472 buffer hits down to 79), which is what earns it the seconds.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedProbeFleet(db.conn);
    try seedRunnersWithLeases(db.conn);

    const plan = try planOf(alloc, db.conn,
        \\WITH page AS (
        \\    SELECT r.id, r.host_id FROM fleet.runners r
        \\    ORDER BY r.created_at DESC, r.id DESC LIMIT 25 OFFSET 0
        \\)
        \\SELECT p.id, EXISTS (
        \\    SELECT 1 FROM fleet.runner_leases l
        \\    WHERE l.runner_id = p.id AND l.status = 'active'
        \\      AND l.lease_expires_at > 1) AS has_live_lease
        \\FROM page p
    );
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, "idx_runner_leases_runner_id_status") == null) {
        std.debug.print("expected a per-page index probe:\n{s}\n", .{plan});
        return error.IndexNotChosen;
    }
    if (std.mem.indexOf(u8, plan, "Seq Scan on runner_leases") != null) {
        std.debug.print("lease table scanned whole, not per page:\n{s}\n", .{plan});
        return error.LeaseTableScannedWhole;
    }
}
