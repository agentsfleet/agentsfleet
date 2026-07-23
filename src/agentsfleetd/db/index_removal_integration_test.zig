//! Integration tier for schema slot 034: the two retired indexes are gone, and
//! the queries that used them still plan against an index.
//!
//! A removal is only safe if the work relocates rather than disappears, so
//! absence alone is not the assertion — each test also reads the plan of the
//! query the dropped index used to serve.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("test_fixtures.zig");
const PgQuery = @import("pg_query.zig").PgQuery;

/// Indexes slot 034 retires. Both are defined in shipped slots (010, 012), so
/// their absence proves the migration ran, not merely that they were never made.
const RETIRED_INDEXES = [_][]const u8{
    "idx_api_keys_key_hash_active",
    "idx_memory_entries_fleet_id",
};

/// Deliberately KEPT: it recorded scans under the workload, so it failed the
/// zero-scan bar. Pinned here so a later cleanup cannot quietly widen the drop.
const KEPT_INDEX = "idx_memory_entries_category";

const KEY_PREFIX = "rmprobe-key-";
const MEM_ID_PREFIX = "rmprobe-mem-";
const FLEET_PROBE = "0195b4ba-8d3a-7f13-8abc-0000000e0001";
const SEED_ROWS: i32 = 20_000;
const MEM_SEED_ROWS: i32 = 40_000;
const PROBE_FLEET_ROWS: i32 = 4_000;

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

fn indexExists(conn: *pg.Conn, name: []const u8) !bool {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::bigint FROM pg_indexes WHERE indexname = $1",
        .{name},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return (try row.get(i64, 0)) > 0;
}

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

test "slot 034 retired exactly the two approved indexes" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    for (RETIRED_INDEXES) |name| {
        if (try indexExists(db.conn, name)) {
            std.debug.print("index {s} still present after slot 034\n", .{name});
            return error.IndexNotRetired;
        }
    }
    // The third candidate stays. Its 4 recorded scans failed the zero-scan bar,
    // and widening the drop on reasoning alone is what the guard exists to stop.
    try std.testing.expect(try indexExists(db.conn, KEPT_INDEX));
}

test "api key auth lookup survives the drop" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    try base.seedTenant(db.conn);
    defer _ = db.conn.exec("DELETE FROM core.api_keys WHERE key_name LIKE $1", .{KEY_PREFIX ++ "%"}) catch |err|
        std.log.warn("api key teardown ignored: {s}", .{@errorName(err)});

    _ = try db.conn.exec(
        \\INSERT INTO core.api_keys
        \\  (uid, tenant_id, key_name, description, key_hash, created_by, active,
        \\   revoked_at, last_used_at, created_at, updated_at)
        \\SELECT overlay(md5('rk' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1::uuid, $2 || g, '', $2 || g, 'seed', TRUE, NULL, NULL, g, 0
        \\FROM generate_series(1, $3::int) g
        \\ON CONFLICT DO NOTHING
    , .{ base.TEST_TENANT_ID, KEY_PREFIX, SEED_ROWS });
    _ = try db.conn.exec("ANALYZE core.api_keys", .{});

    // The dropped index was partial on `active`; the auth path never filters on
    // it, so the unique index on key_hash alone is the one that must serve this.
    const plan = try planOf(alloc, db.conn,
        \\SELECT uid FROM core.api_keys WHERE key_hash = 'rmprobe-key-500'
    );
    defer alloc.free(plan);
    try expectIndex(plan, "api_keys_hash_uniq");
}

test "memory reads relocate onto the composite index" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    defer _ = db.conn.exec("DELETE FROM memory.memory_entries WHERE id LIKE $1", .{MEM_ID_PREFIX ++ "%"}) catch |err|
        std.log.warn("memory teardown ignored: {s}", .{@errorName(err)});

    _ = try db.conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT overlay(md5('rm' || g)::uuid::text placing '7' from 15 for 1)::uuid,
        \\       $1 || g, 'k' || g, 'content', 'core',
        \\       CASE WHEN g <= $3::int THEN $2::uuid
        \\            ELSE md5((g % 200)::text)::uuid END,
        \\       g, g
        \\FROM generate_series(1, $4::int) g
        \\ON CONFLICT DO NOTHING
    , .{ MEM_ID_PREFIX, FLEET_PROBE, PROBE_FLEET_ROWS, MEM_SEED_ROWS });
    _ = try db.conn.exec("ANALYZE memory.memory_entries", .{});

    // The unbounded hydration read is the one that distinguished the two
    // indexes: while the narrow one existed the planner always preferred it,
    // leaving the composite at zero scans. With it gone the same filter is
    // served by the composite — the work relocated, it did not become a scan.
    const plan = try planOf(alloc, db.conn,
        \\SELECT key, content, category FROM memory.memory_entries
        \\WHERE fleet_id = '0195b4ba-8d3a-7f13-8abc-0000000e0001'::uuid
        \\ORDER BY updated_at DESC, id DESC
    );
    defer alloc.free(plan);
    try expectIndex(plan, "idx_memory_entries_fleet_id_updated_at_id");
    if (std.mem.indexOf(u8, plan, "Seq Scan on memory_entries") != null) {
        std.debug.print("filter fell back to a sequential scan:\n{s}\n", .{plan});
        return error.FilterUnindexed;
    }
}
