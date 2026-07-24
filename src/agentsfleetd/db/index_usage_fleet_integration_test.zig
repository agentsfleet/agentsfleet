//! Integration tier for the fleet-scoped half of schema slot 033: the affinity
//! expiry sweep, the workspace event keyset, the reclaim lease lookup, and the
//! fleet list page.
//!
//! Split from `index_usage_integration_test.zig` because every test here needs
//! the same tenant -> workspace -> fleet graph, while that file's tables carry
//! no foreign key chain. Same rule in both: assert the PLAN, and seed enough
//! rows -- and enough DISTINCT key values -- that the index is genuinely the
//! cheaper choice. A fixture where every row shares the probe key proves
//! nothing, because a sequential scan is then correct.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

/// Fleets seeded across the workspace, and the slice the probe key owns. The
/// ratio is what makes an index scan cheaper than a scan-and-filter.
const FLEET_ROWS: i32 = 4_000;
const EVENT_ROWS: i32 = 40_000;
const LEASE_ROWS: i32 = 20_000;
const PROBE_SLICE: i32 = 200;

const WS_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0001";
const FLEET_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0002";
const RUNNER_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000c0003";
const NAME_PREFIX = "idxprobe-fleet-";

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

fn expectIndex(plan: []const u8, index_name: []const u8) !void {
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

fn expectNoSort(plan: []const u8) !void {
    if (std.mem.indexOf(u8, plan, "Sort") != null) {
        std.debug.print("expected no Sort node in plan:\n{s}\n", .{plan});
        return error.PlanSorts;
    }
}

/// Tenant, workspace, the probe runner, and FLEET_ROWS fleets. The probe fleet
/// is one named row among them so a fleet-keyed lookup has something to be
/// selective against.
fn seedGraph(conn: *pg.Conn) !void {
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
        \\  (id, workspace_id, name, source_markdown, config_json, status,
        \\   created_at, updated_at)
        \\SELECT CASE WHEN g = 1 THEN $1::uuid
        \\            ELSE overlay(md5('f' || g)::uuid::text placing '7' from 15 for 1)::uuid END,
        \\       $2::uuid, $3 || g, '', '{}'::jsonb, 'active', 1750000000000 + g, 0
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_PROBE, WS_PROBE, NAME_PREFIX, FLEET_ROWS });
}

/// One affinity row per fleet -- `uq_runner_affinity_fleet_id` allows no more.
/// Only PROBE_SLICE of them point at the probe runner, which is the selectivity
/// the sweep's `last_runner_id` filter depends on.
fn seedAffinity(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, created_at, updated_at)
        \\SELECT overlay(md5('a' || f.id::text)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       f.id,
        \\       CASE WHEN row_number() OVER (ORDER BY f.created_at) <= $2::int
        \\            THEN $1::uuid ELSE NULL END,
        \\       1, 9999999999999, 0, 0, 0, 0, 0, 0
        \\FROM core.fleets f
        \\WHERE f.workspace_id = $3::uuid
        \\ON CONFLICT DO NOTHING
    , .{ RUNNER_PROBE, PROBE_SLICE, WS_PROBE });
    _ = try conn.exec("ANALYZE fleet.runner_affinity", .{});
}

/// Leases spread over the workspace's fleets, with the probe fleet holding a
/// small slice so the reclaim lookup's `fleet_id` filter is selective.
fn seedLeases(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, fencing_token, lease_expires_at, status,
        \\   created_at, updated_at)
        \\SELECT overlay(md5('l' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid,
        \\       CASE WHEN g <= $2::int THEN $3::uuid
        \\            ELSE overlay(md5('f' || (g % ($4::int - 1) + 2))::uuid::text placing '7' from 15 for 1)::uuid END,
        \\       $5::uuid, $6::uuid, 'e' || g, 'actor', 'fleet.run', '{}', 0,
        \\       'standard', 'anthropic', 'claude', 0, 0, 0, 0, g, 9999999999999,
        \\       'active', 0, 0
        \\FROM generate_series(1, $7::int) g
        \\ON CONFLICT DO NOTHING
    , .{ RUNNER_PROBE, PROBE_SLICE, FLEET_PROBE, FLEET_ROWS, WS_PROBE, base.TEST_TENANT_ID, LEASE_ROWS });
    _ = try conn.exec("ANALYZE fleet.runner_leases", .{});
}

/// Events across the workspace's fleets, each with a distinct `created_at` so
/// the keyset cursor has a real ordering to seek into.
fn seedEvents(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\SELECT overlay(md5('e' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, 'evt-' || g, $2::uuid, 'actor', 'fleet.run', 'ok',
        \\       '{}'::jsonb, 1750000000000 + g, 0
        \\FROM generate_series(1, $3::int) g
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_PROBE, WS_PROBE, EVENT_ROWS });
    _ = try conn.exec("ANALYZE core.fleet_events", .{});
}

/// Teardown runs child-first: `core.fleets` is not cascade-backed on
/// `workspace_id`, so a lingering fleet blocks the workspace DELETE.
fn teardown(conn: *pg.Conn) void {
    base.teardownFleets(conn, WS_PROBE);
    base.teardownWorkspace(conn, WS_PROBE);
    _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_PROBE}) catch |err|
        std.log.warn("runner wipe ignored: {s}", .{@errorName(err)});
}

test "affinity expiry sweep is planned as an index scan" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedLeases(db.conn);
    try seedAffinity(db.conn);

    // The sweep's own statement, verbatim in shape: this is the read that runs
    // once per due runner per cycle, so it is the whole reason slot 033 is P1.
    const plan = try planOf(alloc, db.conn,
        \\UPDATE fleet.runner_affinity a
        \\SET leased_until = 1, updated_at = 2
        \\WHERE a.last_runner_id = '0195b4ba-8d3a-7f13-8abc-0000000c0003'::uuid
        \\  AND a.leased_until > 1
        \\  AND a.fleet_id IN (
        \\    SELECT l.fleet_id FROM fleet.runner_leases l
        \\    WHERE l.runner_id = '0195b4ba-8d3a-7f13-8abc-0000000c0003'::uuid
        \\      AND l.status = 'active')
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_runner_affinity_last_runner_id_leased_until");
}

test "reclaim lease lookup is a single index seek" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedLeases(db.conn);

    const plan = try planOf(alloc, db.conn,
        \\SELECT id FROM fleet.runner_leases
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000c0002'::uuid
        \\  AND status = 'active'
        \\ORDER BY fencing_token DESC LIMIT 1
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_runner_leases_fleet_id_status_fencing_token");
    try expectNoSort(plan);
}

test "workspace event keyset is an index seek" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedEvents(db.conn);

    const plan = try planOf(alloc, db.conn,
        \\SELECT event_id, actor FROM core.fleet_events
        \\WHERE workspace_id = '0195b4ba-8d3a-7f13-8abc-0000000c0001'::uuid
        \\  AND (created_at < 1750000030000
        \\       OR (created_at = 1750000030000 AND event_id < 'evt-9'))
        \\ORDER BY created_at DESC, event_id DESC
        \\LIMIT 50
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_fleet_events_workspace_id_created_at_event_id");
    try expectNoSort(plan);
}

test "runner list liveness is bounded by page size" {
    // The restructured list evaluates the lease-liveness EXISTS over the page,
    // not the table. The observable difference is which side of the join the
    // lease table is read from: the original shape made PostgreSQL hash the
    // WHOLE of `runner_leases` once per request (a Seq Scan on it, 6 468 buffer
    // hits against 200k rows), while the page-scoped form does page-size index
    // lookups (79). Asserting the absence of that seq scan is what pins the fix.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    // A fleet of runners, each holding its own lease. The shared `seedLeases`
    // fixture points every lease at one runner, which gives the planner no
    // reason to prefer a per-runner lookup -- correct for that fixture, wrong
    // for this question.
    _ = try db.conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels,
        \\   last_seen_at, created_at, updated_at)
        \\SELECT overlay(md5('lr' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, $1 || g, 'standard', 'active', '[]'::jsonb, 0, 1750000000000 + g, 0
        \\FROM generate_series(1, 5000) g
        \\ON CONFLICT DO NOTHING
    , .{NAME_PREFIX});
    _ = try db.conn.exec(
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
        \\FROM generate_series(1, 5000) g
        \\ON CONFLICT DO NOTHING
    , .{ FLEET_PROBE, WS_PROBE, base.TEST_TENANT_ID });
    _ = try db.conn.exec("ANALYZE fleet.runners", .{});
    _ = try db.conn.exec("ANALYZE fleet.runner_leases", .{});

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
    try expectIndex(plan, "idx_runner_leases_runner_id_status");
    if (std.mem.indexOf(u8, plan, "Seq Scan on runner_leases") != null) {
        std.debug.print("lease table scanned whole, not per page:\n{s}\n", .{plan});
        return error.LeaseTableScannedWhole;
    }
}
