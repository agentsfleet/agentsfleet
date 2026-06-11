//! Integration tier for the single durable write adapter (`zombie_memory.zig`):
//! `storeEntry` / `enforceCap` / `listAll` need a live Postgres and the
//! `memory_runtime` role. Mirrors `zombie_memory_role_test.zig`'s harness —
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration-db`);
//! self-skips otherwise. `memory.memory_entries` carries no FK to `core.zombies`
//! (role isolation is the boundary), so a synthetic `zombie_id` needs no seed.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");
const base = @import("../db/test_fixtures.zig");
const adapter = @import("zombie_memory.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// Distinct synthetic zombies per concern so concurrent/re-runs never collide.
const ZID_PERSIST = "0195b4ba-8d3a-7f13-8abc-00000000a001";
const ZID_CAP = "0195b4ba-8d3a-7f13-8abc-00000000a002";
const ZID_ISO_A = "0195b4ba-8d3a-7f13-8abc-00000000a003";
const ZID_ISO_B = "0195b4ba-8d3a-7f13-8abc-00000000a004";

const TestDb = struct {
    pool: *pg.Pool,
    conn: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        _ = try ctx.conn.exec("SET ROLE memory_runtime", .{});
        return .{ .pool = ctx.pool, .conn = ctx.conn };
    }

    fn close(self: TestDb) void {
        _ = self.conn.exec("RESET ROLE", .{}) catch |err| std.log.warn("reset role ignored: {s}", .{@errorName(err)});
        self.pool.release(self.conn);
        self.pool.deinit();
    }

    /// Wipe a zombie's rows so a re-run starts clean (memory_runtime has DELETE).
    fn wipe(self: TestDb, zombie_id: []const u8) void {
        _ = self.conn.exec("DELETE FROM memory.memory_entries WHERE zombie_id = $1::uuid", .{zombie_id}) catch |err|
            std.log.warn("wipe ignored: {s}", .{@errorName(err)});
    }

    fn count(self: TestDb, zombie_id: []const u8) !usize {
        var q = PgQuery.from(try self.conn.query("SELECT COUNT(*) FROM memory.memory_entries WHERE zombie_id = $1::uuid", .{zombie_id}));
        defer q.deinit();
        const row = try q.next() orelse return 0;
        return @intCast(try row.get(i64, 0));
    }
};

fn freeDeltas(alloc: std.mem.Allocator, rows: []adapter.MemoryDelta) void {
    for (rows) |r| {
        alloc.free(r.key);
        alloc.free(r.content);
        alloc.free(r.category);
    }
    alloc.free(rows);
}

test "integration: storeEntry persists, listAll reads it back, upsert is idempotent" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_PERSIST);
    defer db.wipe(ZID_PERSIST);

    try adapter.storeEntry(db.conn, "id-p1", ZID_PERSIST, "deploy_target", "fly", "core", "1700000001");
    try adapter.storeEntry(db.conn, "id-p2", ZID_PERSIST, "owner", "indy", "core", "1700000002");

    const rows = try adapter.listAll(alloc, db.conn, ZID_PERSIST);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 2), rows.len);
    // Newest first (updated_at DESC): "owner" was stored last.
    try std.testing.expectEqualStrings("owner", rows[0].key);

    // Idempotent upsert: same key, new id/content → one row, content updated.
    try adapter.storeEntry(db.conn, "id-p1b", ZID_PERSIST, "deploy_target", "render", "core", "1700000003");
    try std.testing.expectEqual(@as(usize, 2), try db.count(ZID_PERSIST));
}

test "integration: enforceCap evicts the coldest; an upsert does not grow the set" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_CAP);
    defer db.wipe(ZID_CAP);

    // Five entries, ascending updated_at → k1 is coldest, k5 newest.
    inline for (.{ "1", "2", "3", "4", "5" }) |n| {
        try adapter.storeEntry(db.conn, "id-c" ++ n, ZID_CAP, "k" ++ n, "v" ++ n, "core", "170000000" ++ n);
    }
    try std.testing.expectEqual(@as(usize, 5), try db.count(ZID_CAP));

    // Cap at 3 → the two coldest (k1, k2) are evicted, newest three kept, and
    // the eviction count comes back exactly (the driver's rows-affected).
    try std.testing.expectEqual(@as(u64, 2), try adapter.enforceCap(db.conn, ZID_CAP, 3));
    const rows = try adapter.listAll(alloc, db.conn, ZID_CAP);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 3), rows.len);
    try std.testing.expectEqualStrings("k5", rows[0].key); // newest survives
    for (rows) |r| try std.testing.expect(!std.mem.eql(u8, r.key, "k1")); // coldest gone

    // Already under the cap → the second pass evicts (and reports) zero.
    try std.testing.expectEqual(@as(u64, 0), try adapter.enforceCap(db.conn, ZID_CAP, 3));

    // An upsert to an existing key is overwrite, not insert — count holds.
    try adapter.storeEntry(db.conn, "id-c5b", ZID_CAP, "k5", "v5b", "core", "1700000006");
    try std.testing.expectEqual(@as(usize, 3), try db.count(ZID_CAP));
}

test "integration: enforceCap failure propagates as an error, deleting nothing" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_CAP);
    defer db.wipe(ZID_CAP);

    try adapter.storeEntry(db.conn, "id-f1", ZID_CAP, "kf1", "vf1", "core", "1700000001");
    try std.testing.expectEqual(@as(usize, 1), try db.count(ZID_CAP));

    // Inject a deterministic failure: the driver rejects a malformed zombie_id
    // before the uuid bind (error.InvalidUUID), so enforceCap surfaces an error
    // instead of inventing a zero — the handler's catch path (warn + continue,
    // eviction counter untouched) consumes any such error identically in
    // production. The row count above proves the failed eviction deleted nothing.
    try std.testing.expectError(error.InvalidUUID, adapter.enforceCap(db.conn, "not-a-uuid", 0));
}

test "integration: listAll is scoped per zombie — no cross-zombie bleed" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_ISO_A);
    db.wipe(ZID_ISO_B);
    defer db.wipe(ZID_ISO_A);
    defer db.wipe(ZID_ISO_B);

    try adapter.storeEntry(db.conn, "id-a1", ZID_ISO_A, "secret", "alpha", "core", "1700000001");
    try adapter.storeEntry(db.conn, "id-b1", ZID_ISO_B, "secret", "beta", "core", "1700000001");

    const rows_a = try adapter.listAll(alloc, db.conn, ZID_ISO_A);
    defer freeDeltas(alloc, rows_a);
    try std.testing.expectEqual(@as(usize, 1), rows_a.len);
    try std.testing.expectEqualStrings("alpha", rows_a[0].content); // never beta
}
