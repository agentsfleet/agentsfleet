//! Integration tier for the fleet-scoped half of schema slot 033: the affinity
//! expiry sweep, the reclaim lease lookup, and the workspace event keyset.
//!
//! WHAT IS ACTUALLY UNDER TEST. Two things, and both are ours: the index's
//! column list and order, and the query's WHERE/ORDER BY. Whether PostgreSQL's
//! cost model prefers that index over a sequential scan at some row count is
//! PostgreSQL's behaviour, not ours — and reproducing its crossover took tens of
//! thousands of seeded rows per test, which is what made this suite the slowest
//! thing in the integration lane.
//!
//! So each index is checked two ways, cheaply:
//!
//!   1. `expectIndexShape` reads the definition back from `pg_indexes` and pins
//!      the exact columns and directions. Catches a dropped, renamed, reordered
//!      or wrong-direction index, and needs no rows at all.
//!   2. `expectServesOrdering` plans the real query with sequential scans
//!      disabled and asserts our index is chosen with no Sort node. That answers
//!      "can this index serve this query's filter and ordering" — the part our
//!      code decides — independently of table size.
//!
//! What this deliberately no longer asserts is that the planner PREFERS the
//! index at scale. That guards against one thing: a competing index shadowing
//! ours. After slot 034 retired the two overlapping indexes, no surviving index
//! on these tables shares a leading column with any of slot 033's, so there is
//! nothing left to do the shadowing. The one case where shadowing really did
//! happen — the memory composite sitting at zero scans behind a narrower index —
//! keeps its full-size assertion in `index_removal_integration_test.zig`, where
//! that claim is load-bearing for the drop.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

/// Enough rows for the query to be planned and for the fixture to be legible;
/// no longer sized to move PostgreSQL's cost model, because these assertions no
/// longer depend on it.
const FLEET_ROWS: i32 = 20;
const EVENT_ROWS: i32 = 200;
const LEASE_ROWS: i32 = 200;
const PROBE_SLICE: i32 = 20;

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

/// The index exists and indexes exactly `want_columns`, in that order and those
/// directions. Reads the definition back from the catalog, so a reorder or a
/// dropped DESC fails here rather than silently changing which reads are served.
fn expectIndexShape(alloc: std.mem.Allocator, conn: *pg.Conn, name: []const u8, want_columns: []const u8) !void {
    var q = PgQuery.from(try conn.query(
        "SELECT indexdef FROM pg_indexes WHERE indexname = $1",
        .{name},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse {
        std.debug.print("index {s} does not exist\n", .{name});
        return error.IndexMissing;
    };
    const def = try alloc.dupe(u8, try row.get([]const u8, 0));
    defer alloc.free(def);
    const open = std.mem.lastIndexOfScalar(u8, def, '(') orelse return error.IndexDefUnparsed;
    const close = std.mem.lastIndexOfScalar(u8, def, ')') orelse return error.IndexDefUnparsed;
    const got = def[open + 1 .. close];
    if (!std.mem.eql(u8, got, want_columns)) {
        std.debug.print("index {s} columns:\n  want: {s}\n  got:  {s}\n", .{ name, want_columns, got });
        return error.IndexShapeChanged;
    }
}

/// `index_name` can serve `sql`'s filter AND its ordering: with sequential scans
/// disabled the planner reaches for it and needs no Sort node. Disabling seqscan
/// is what makes this independent of table size — the question asked is whether
/// the index FITS the query, not whether PostgreSQL's cost model prefers it at
/// some particular row count.
fn expectServesOrdering(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8, index_name: []const u8) !void {
    _ = try conn.exec("SET enable_seqscan = off", .{});
    defer _ = conn.exec("RESET enable_seqscan", .{}) catch |err|
        std.log.warn("reset enable_seqscan ignored: {s}", .{@errorName(err)});

    const plan = try planOf(alloc, conn, sql);
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
    if (std.mem.indexOf(u8, plan, "Sort") != null) {
        std.debug.print("index {s} did not supply the ordering:\n{s}\n", .{ index_name, plan });
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
/// Only PROBE_SLICE of them point at the probe runner.
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
/// slice so the reclaim lookup's `fleet_id` filter has something to select.
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

/// Events for the probe fleet, each with a distinct `created_at` so the keyset
/// cursor has a real ordering to seek into.
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

test "affinity expiry sweep is served by its index" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedLeases(db.conn);
    try seedAffinity(db.conn);

    try expectIndexShape(alloc, db.conn, "idx_runner_affinity_last_runner_id_leased_until", "last_runner_id, leased_until");

    // The sweep's own filter: this runs once per due runner per cycle, which is
    // why `last_runner_id` needed indexing at all.
    try expectServesOrdering(alloc, db.conn,
        \\SELECT a.fleet_id FROM fleet.runner_affinity a
        \\WHERE a.last_runner_id = '0195b4ba-8d3a-7f13-8abc-0000000c0003'::uuid
        \\  AND a.leased_until > 1
        \\ORDER BY a.last_runner_id, a.leased_until
    , "idx_runner_affinity_last_runner_id_leased_until");
}

test "reclaim lease lookup is served by its index" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedLeases(db.conn);

    try expectIndexShape(alloc, db.conn, "idx_runner_leases_fleet_id_status_fencing_token", "fleet_id, status, fencing_token DESC");

    // Filter, ordering and LIMIT 1 in one seek — the trailing fencing_token is
    // what removes the sort, so a Sort node here means the index lost its point.
    try expectServesOrdering(alloc, db.conn,
        \\SELECT id FROM fleet.runner_leases
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000c0002'::uuid
        \\  AND status = 'active'
        \\ORDER BY fencing_token DESC LIMIT 1
    , "idx_runner_leases_fleet_id_status_fencing_token");
}

test "workspace event keyset is served by its index" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer teardown(db.conn);
    try seedGraph(db.conn);
    try seedEvents(db.conn);

    try expectIndexShape(alloc, db.conn, "idx_fleet_events_workspace_id_created_at_event_id", "workspace_id, created_at DESC, event_id DESC");

    // The trailing event_id is the keyset tiebreak; without it the cursor
    // comparison becomes a post-filter on every page.
    try expectServesOrdering(alloc, db.conn,
        \\SELECT event_id, actor FROM core.fleet_events
        \\WHERE workspace_id = '0195b4ba-8d3a-7f13-8abc-0000000c0001'::uuid
        \\  AND (created_at < 1750000030000
        \\       OR (created_at = 1750000030000 AND event_id < 'evt-9'))
        \\ORDER BY created_at DESC, event_id DESC
        \\LIMIT 50
    , "idx_fleet_events_workspace_id_created_at_event_id");
}
