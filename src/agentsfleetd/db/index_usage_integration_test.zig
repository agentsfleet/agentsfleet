//! Integration tier for schema slot 033: every index it adds must be CHOSEN BY
//! THE PLANNER for the query that justifies it.
//!
//! Asserting the index exists would pass on a merely-created index that the
//! planner never picks — indistinguishable from a fix, in a green suite. So the
//! read test here reads an `EXPLAIN` plan and asserts on the node, and seeds
//! enough rows AND enough distinct key values that an index scan is genuinely
//! the cheaper plan. Under-seeding is the failure mode to watch: on a small or
//! single-valued table a sequential scan IS correct, and the test would fail for
//! the wrong reason.
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
const protocol = @import("contract").protocol;

/// The migration slot this suite covers.
const SLOT_VERSION: i32 = 33;

/// Rows that make an index scan cheaper than a sequential scan on these tables.
const MEM_SEED_ROWS: u32 = 40_000;

/// Rows belonging to the probe fleet — exactly the per-fleet cap production
/// enforces, so the fixture cannot describe a fleet that could not exist. Small
/// against MEM_SEED_ROWS, which is what makes the fleet a selective slice.
const PROBE_FLEET_ROWS: i32 = @intCast(protocol.MAX_MEMORY_ENTRIES_PER_AGENT);

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

/// The planner chose `index_name` — not merely that the index exists.
fn expectIndex(plan: []const u8, index_name: []const u8) !void {
    if (std.mem.indexOf(u8, plan, index_name) == null) {
        std.debug.print("expected index {s} in plan:\n{s}\n", .{ index_name, plan });
        return error.IndexNotChosen;
    }
}

/// No Sort node: the index supplied the ordering rather than the executor.
fn expectNoSort(plan: []const u8) !void {
    if (std.mem.indexOf(u8, plan, "Sort") != null) {
        std.debug.print("expected no Sort node in plan:\n{s}\n", .{plan});
        return error.PlanSorts;
    }
}

/// Seed memory across MANY fleets, with the probe fleet a small slice of the
/// table. A single-fleet seed would be the wrong shape: if every row matches
/// the filter, a sequential scan really is the cheaper plan and the planner is
/// right to take it. Hydration is per-fleet on a table holding every fleet, so
/// the fixture has to reproduce that selectivity to prove anything.
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

test "bounded memory read is index-ordered" {
    // The composite index earns its ordering only where the plan can exit early.
    // `fleet_memory.listAll` fetches a fleet's WHOLE memory set with no LIMIT, and
    // for an unbounded fetch PostgreSQL correctly prefers bitmap-scan + sort: an
    // ordered index scan buys nothing when every row is returned anyway, and costs
    // random heap access. Measured on a 4000-of-40000 fixture, both with and
    // without the narrow `idx_memory_entries_fleet_id` present.
    //
    // So this asserts the bounded shape, which is what the index actually serves.
    // The unbounded read's own guarantee — that it still reaches the composite
    // rather than a sequential scan once slot 034 drops the narrow index — is
    // asserted in `index_removal_integration_test.zig`.
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer wipeMemory(db.conn);
    try seedMemory(db.conn, MEM_SEED_ROWS);

    const plan = try planOf(alloc, db.conn,
        \\SELECT key, content, category
        \\FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000b0002'::uuid
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 50
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_memory_entries_fleet_id_updated_at_id");
    try expectNoSort(plan);
}
