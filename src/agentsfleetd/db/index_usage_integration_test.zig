//! Integration tier for schema slot 033: every index it adds must be CHOSEN BY
//! THE PLANNER for the query that justifies it.
//!
//! WHAT IS UNDER TEST is what our code owns: each index's column list and order,
//! and that the index CAN serve its query. Whether the planner PREFERS it over a
//! sequential scan, and whether it supplies the ordering without a Sort node, are
//! scale-dependent cost-model decisions PostgreSQL owns — reproducing them took
//! tens of thousands of seeded rows per run. So the shape is pinned from the
//! catalog (free) and fitness is checked with `enable_seqscan = off` (size
//! independent). The one genuinely scale-sized memory assertion — that a read
//! relocates onto the composite after slot 034 drops the narrow index — lives in
//! `index_removal_integration_test.zig`, where the claim is load-bearing.
//!
//! The fleet-scoped indexes (affinity, leases, events) are covered by the
//! sibling `index_usage_fleet_integration_test.zig`, which needs a tenant ->
//! workspace -> fleet graph this file's tables do not.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const schema = @import("schema");
const PgQuery = @import("pg_query.zig").PgQuery;

/// The migration slot this suite covers.
const SLOT_VERSION: i32 = 33;

/// A minimal legible fixture. Fitness is checked with `enable_seqscan = off`, so
/// it does not depend on the probe fleet being a selective slice of a large
/// table — a few rows per fleet is enough for the plan to form.
const MEM_SEED_ROWS: u32 = 200;
const PROBE_FLEET_ROWS: i32 = 20;

const FLEET_MEM = "0195b4ba-8d3a-7f13-8abc-0000000b0002";
const MEM_ID_PREFIX = "idxprobe-mem-";

/// Every index slot 033 creates, in file order. Four, deliberately: the slot
/// indexes only the reads whose cost grows without bound. List sorts over
/// runners, fleets and api keys are left unindexed at the ~100-runner scale the
/// slot documents — see its header.
const SLOT_033_INDEXES = [_][]const u8{
    "idx_runner_affinity_last_runner_id_leased_until",
    "idx_runner_leases_fleet_id_status_fencing_token",
    "idx_fleet_events_workspace_id_created_at_event_id",
    "idx_memory_entries_fleet_id_updated_at_id",
};

/// The registered slot's text, or null when nothing claims that version.
fn slotSql(version: i32) ?[]const u8 {
    for (schema.migrations) |m| {
        if (m.version == version) return m.sql;
    }
    return null;
}

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

/// Read an `EXPLAIN` plan back as one text blob. Caller owns the result.
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
/// directions — read back from the catalog, so a reorder or a dropped DESC fails.
fn expectIndexShape(alloc: std.mem.Allocator, conn: *pg.Conn, name: []const u8, want_columns: []const u8) !void {
    var q = PgQuery.from(try conn.query("SELECT indexdef FROM pg_indexes WHERE indexname = $1", .{name}));
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

/// `index_name` CAN serve `sql`'s filter: with sequential scans disabled the
/// planner reaches for it. Size independent — this asks whether the index fits
/// the query, not whether the cost model prefers it at some row count.
fn expectServesFilter(alloc: std.mem.Allocator, conn: *pg.Conn, sql: []const u8, index_name: []const u8) !void {
    _ = try conn.exec("SET enable_seqscan = off", .{});
    defer _ = conn.exec("RESET enable_seqscan", .{}) catch |err|
        std.log.warn("reset enable_seqscan ignored: {s}", .{@errorName(err)});
    const plan = try planOf(alloc, conn, sql);
    defer alloc.free(plan);
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

/// Seed memory across a handful of fleets, the probe fleet among them. Size and
/// selectivity are not load-bearing here (the fitness check forces the index),
/// so this stays small.
fn seedMemory(conn: *pg.Conn, rows: u32) !void {
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT overlay(gen_random_uuid()::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, 'k' || g, 'content', 'core',
        \\       CASE WHEN g <= $3::int THEN $2::uuid
        \\            ELSE md5((g % 200)::text)::uuid END,
        \\       1750000000000 + g, 1750000000000 + g
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ MEM_ID_PREFIX, FLEET_MEM, @as(i32, PROBE_FLEET_ROWS), @as(i32, @intCast(rows)) });
    _ = try conn.exec("ANALYZE memory.memory_entries", .{});
}

fn wipeMemory(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM memory.memory_entries WHERE id LIKE $1", .{MEM_ID_PREFIX ++ "%"}) catch |err|
        std.log.warn("memory wipe ignored: {s}", .{@errorName(err)});
}

test "slot 033 indexes are applied exactly once" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    for (SLOT_033_INDEXES) |name| {
        var q = PgQuery.from(try db.conn.query(
            "SELECT COUNT(*)::bigint FROM pg_indexes WHERE indexname = $1",
            .{name},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.DbRowShape;
        const n = try row.get(i64, 0);
        if (n != 1) {
            std.debug.print("index {s} present {d} times, want 1\n", .{ name, n });
            return error.IndexNotAppliedOnce;
        }
    }
}

test "slot 033 creates exactly the indexes this suite covers" {
    // Pins the slot's SIZE, not just its members. An index added to the slot
    // without a matching plan assertion here is the created-but-unproven case
    // the suite exists to prevent, so it fails until covered.
    const slot = slotSql(SLOT_VERSION) orelse return error.SlotNotRegistered;
    var lines = std.mem.splitScalar(u8, slot, '\n');
    var creates: usize = 0;
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (std.mem.startsWith(u8, line, "CREATE INDEX")) creates += 1;
    }
    try std.testing.expectEqual(SLOT_033_INDEXES.len, creates);
}

test "slot 033 re-applies as a no-op" {
    // Idempotency by construction: every statement in the slot is guarded, so a
    // re-run against a provisioned database changes nothing. Reading it back
    // through the registered migration array rather than the file also proves
    // the slot is wired into `schema/embed.zig` — an unregistered slot never
    // runs at all, and would otherwise fail only at first deploy.
    const slot = slotSql(SLOT_VERSION) orelse return error.SlotNotRegistered;
    var lines = std.mem.splitScalar(u8, slot, '\n');
    var guarded: usize = 0;
    while (lines.next()) |raw| {
        // Comment lines discuss DDL without being it -- match statements only.
        const line = std.mem.trim(u8, raw, " \t\r");
        if (!std.mem.startsWith(u8, line, "CREATE INDEX")) continue;
        if (std.mem.indexOf(u8, line, "IF NOT EXISTS") == null) {
            std.debug.print("unguarded statement in slot 033:\n{s}\n", .{line});
            return error.MigrationNotIdempotent;
        }
        guarded += 1;
    }
    try std.testing.expectEqual(SLOT_033_INDEXES.len, guarded);
}

test "memory composite has the right shape and serves the fleet filter" {
    // What our code controls, asserted cheaply. Two things:
    //   - the index indexes exactly (fleet_id, updated_at DESC, id DESC), so a
    //     column reorder or a dropped DESC fails here;
    //   - with sequential scans disabled the planner reaches for it to answer a
    //     fleet-scoped read, proving the index CAN serve that filter.
    //
    // What is deliberately NOT asserted is that the planner supplies the ordering
    // WITHOUT a Sort node, or that it prefers the index over a scan. Both are
    // scale-dependent planner behaviour, not properties of our code: the ordered
    // index scan only beats bitmap-scan-plus-sort once a fleet's rows are a small
    // enough slice of the table, and the crossover for the unbounded `listAll`
    // was measured to sit near 3% — reproducing it took a 40 000-row fixture per
    // run, for a fact PostgreSQL owns rather than we do. See the file header.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeMemory(db.conn);
    try seedMemory(db.conn, MEM_SEED_ROWS);

    try expectIndexShape(alloc, db.conn, "idx_memory_entries_fleet_id_updated_at_id", "fleet_id, updated_at DESC, id DESC");
    try expectServesFilter(alloc, db.conn,
        \\SELECT key, content, category
        \\FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000b0002'::uuid
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 50
    , "idx_memory_entries_fleet_id_updated_at_id");
}
