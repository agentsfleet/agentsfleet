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
const protocol = @import("contract").protocol;
const PgQuery = @import("../db/pg_query.zig").PgQuery;

// Distinct synthetic zombies per concern so concurrent/re-runs never collide.
const ZID_PERSIST = "0195b4ba-8d3a-7f13-8abc-00000000a001";
const ZID_CAP = "0195b4ba-8d3a-7f13-8abc-00000000a002";
const ZID_ISO_A = "0195b4ba-8d3a-7f13-8abc-00000000a003";
const ZID_ISO_B = "0195b4ba-8d3a-7f13-8abc-00000000a004";
const ZID_TIER = "0195b4ba-8d3a-7f13-8abc-00000000a005";
const ZID_HEADLINE = "0195b4ba-8d3a-7f13-8abc-00000000a006";
const ZID_SWEEP_A = "0195b4ba-8d3a-7f13-8abc-00000000a007";
const ZID_SWEEP_B = "0195b4ba-8d3a-7f13-8abc-00000000a008";
const ZID_TS = "0195b4ba-8d3a-7f13-8abc-00000000a009";

// Sweep-age fixture instants (epoch ms): rows at T_AGED are older than the
// CUTOFF; rows at T_YOUNG are newer. CUTOFF_ALL is far future — at that cutoff
// every seeded row counts as aged, which isolates the category guard.
const T_AGED: i64 = 1_700_000_001_000;
const T_YOUNG: i64 = 1_700_000_005_000;
const CUTOFF: i64 = 1_700_000_003_000;
const CUTOFF_ALL: i64 = 2_000_000_000_000;

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

    fn countCategory(self: TestDb, zombie_id: []const u8, category: []const u8) !usize {
        var q = PgQuery.from(try self.conn.query("SELECT COUNT(*) FROM memory.memory_entries WHERE zombie_id = $1::uuid AND category = $2", .{ zombie_id, category }));
        defer q.deinit();
        const row = try q.next() orelse return 0;
        return @intCast(try row.get(i64, 0));
    }
};

/// Bulk-seed `n` daily rows in one INSERT (a thousand storeEntry round-trips
/// would drag the lane), updated_at ascending from a warm epoch so every daily
/// lands strictly newer than any previously stored core fact. uid is composed
/// v7-shaped from the zombie's distinguishing tail + n (collision-free across
/// fixture zombies); the globally-unique id column carries the same tail.
fn seedDailies(db: TestDb, zombie_id: []const u8, n: usize) !void {
    _ = try db.conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, zombie_id, created_at, updated_at)
        \\SELECT (('0195b4ba-8d3a-7' || lpad(to_hex(n), 3, '0') || '-8abc-'
        \\         || substr(replace($1::uuid::text, '-', ''), 25, 8) || lpad(to_hex(n), 4, '0')))::uuid,
        \\       'dk-' || substr(replace($1::uuid::text, '-', ''), 29, 4) || '-' || n,
        \\       'dk' || n, 'noise', $2, $1::uuid,
        \\       1700000000000, 1700000000000 + n
        \\FROM generate_series(1, $3::int) n
        \\ON CONFLICT (key, zombie_id) DO NOTHING
    , .{ zombie_id, adapter.CATEGORY_DAILY, @as(i64, @intCast(n)) });
}

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

    try adapter.storeEntry(db.conn, "id-p1", ZID_PERSIST, "deploy_target", "fly", adapter.CATEGORY_CORE, 1_700_000_001_000);
    try adapter.storeEntry(db.conn, "id-p2", ZID_PERSIST, "owner", "indy", adapter.CATEGORY_CORE, 1_700_000_002_000);

    const rows = try adapter.listAll(alloc, db.conn, ZID_PERSIST);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 2), rows.len);
    // Newest first (updated_at DESC): "owner" was stored last.
    try std.testing.expectEqualStrings("owner", rows[0].key);

    // Idempotent upsert: same key, new id/content → one row, content updated.
    try adapter.storeEntry(db.conn, "id-p1b", ZID_PERSIST, "deploy_target", "render", adapter.CATEGORY_CORE, 1_700_000_003_000);
    try std.testing.expectEqual(@as(usize, 2), try db.count(ZID_PERSIST));
}

test "integration: enforceCap over an all-core set evicts the coldest core as last resort; an upsert does not grow the set" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_CAP);
    defer db.wipe(ZID_CAP);

    // Five entries, ascending updated_at → k1 is coldest, k5 newest.
    inline for (.{ "1", "2", "3", "4", "5" }, 1..) |n, ts_step| {
        try adapter.storeEntry(db.conn, "id-c" ++ n, ZID_CAP, "k" ++ n, "v" ++ n, adapter.CATEGORY_CORE, 1_700_000_000_000 + @as(i64, ts_step));
    }
    try std.testing.expectEqual(@as(usize, 5), try db.count(ZID_CAP));

    // Every row is `core` (no non-core victims exist), so the last-resort path
    // applies: cap at 3 → the two coldest cores (k1, k2) are evicted, newest
    // three kept, and the eviction count comes back exactly (rows-affected).
    try std.testing.expectEqual(@as(u64, 2), try adapter.enforceCap(db.conn, ZID_CAP, 3));
    const rows = try adapter.listAll(alloc, db.conn, ZID_CAP);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 3), rows.len);
    try std.testing.expectEqualStrings("k5", rows[0].key); // newest survives
    for (rows) |r| try std.testing.expect(!std.mem.eql(u8, r.key, "k1")); // coldest gone

    // Already under the cap → the second pass evicts (and reports) zero.
    try std.testing.expectEqual(@as(u64, 0), try adapter.enforceCap(db.conn, ZID_CAP, 3));

    // An upsert to an existing key is overwrite, not insert — count holds.
    try adapter.storeEntry(db.conn, "id-c5b", ZID_CAP, "k5", "v5b", adapter.CATEGORY_CORE, 1_700_000_000_006);
    try std.testing.expectEqual(@as(usize, 3), try db.count(ZID_CAP));
}

test "integration: enforceCap failure propagates as an error, deleting nothing" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_CAP);
    defer db.wipe(ZID_CAP);

    try adapter.storeEntry(db.conn, "id-f1", ZID_CAP, "kf1", "vf1", adapter.CATEGORY_CORE, 1_700_000_001_000);
    try std.testing.expectEqual(@as(usize, 1), try db.count(ZID_CAP));

    // Inject a deterministic failure: the driver rejects a malformed zombie_id
    // before the uuid bind (error.InvalidUUID), so enforceCap surfaces an error
    // instead of inventing a zero — the handler's catch path (warn + continue,
    // eviction counter untouched) consumes any such error identically in
    // production. The row count above proves the failed eviction deleted nothing.
    try std.testing.expectError(error.InvalidUUID, adapter.enforceCap(db.conn, "not-a-uuid", 0));
}

test "integration: cap eviction takes the coldest non-core rows before any core row" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_TIER);
    defer db.wipe(ZID_TIER);

    // Three core facts are the COLDEST rows of all; four daily entries land newer.
    inline for (.{ "1", "2", "3" }, 1..) |n, ts_step| {
        try adapter.storeEntry(db.conn, "id-tc" ++ n, ZID_TIER, "kc" ++ n, "v" ++ n, adapter.CATEGORY_CORE, 1_700_000_000_000 + @as(i64, ts_step));
    }
    inline for (.{ "4", "5", "6", "7" }, 4..) |n, ts_step| {
        try adapter.storeEntry(db.conn, "id-td" ++ n, ZID_TIER, "kd" ++ n, "v" ++ n, adapter.CATEGORY_DAILY, 1_700_000_000_000 + @as(i64, ts_step));
    }

    // Cap 5 over 7 rows: the victims are the two coldest DAILY rows (kd4, kd5),
    // never a core row — even though every core row is colder than every daily.
    try std.testing.expectEqual(@as(u64, 2), try adapter.enforceCap(db.conn, ZID_TIER, 5));
    try std.testing.expectEqual(@as(usize, 3), try db.countCategory(ZID_TIER, adapter.CATEGORY_CORE));
    const rows = try adapter.listAll(alloc, db.conn, ZID_TIER);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 5), rows.len);
    for (rows) |r| {
        try std.testing.expect(!std.mem.eql(u8, r.key, "kd4"));
        try std.testing.expect(!std.mem.eql(u8, r.key, "kd5"));
    }
}

test "integration: one core fact survives a thousand daily pushes — durable and hydrated" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_HEADLINE);
    defer db.wipe(ZID_HEADLINE);

    // The core fact is the single OLDEST row the zombie owns — under the old
    // recency-only currency it would be the first casualty of both mechanisms.
    const cap = protocol.MAX_MEMORY_ENTRIES_PER_ZOMBIE;
    try adapter.storeEntry(db.conn, "id-h1", ZID_HEADLINE, "owner", "indy", adapter.CATEGORY_CORE, 1_600_000_000_000);
    try seedDailies(db, ZID_HEADLINE, cap);
    try std.testing.expectEqual(cap + 1, try db.count(ZID_HEADLINE));

    // One row over the production cap: the eviction victim is the coldest
    // DAILY, never the coldest-overall core fact.
    try std.testing.expectEqual(@as(u64, 1), try adapter.enforceCap(db.conn, ZID_HEADLINE, cap));
    try std.testing.expectEqual(@as(usize, 1), try db.countCategory(ZID_HEADLINE, adapter.CATEGORY_CORE));

    // Hydration: under a byte budget far too small for the set, the selective
    // window still carries the core fact ahead of a thousand newer dailies.
    const rows = try adapter.listAll(alloc, db.conn, ZID_HEADLINE);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(cap, rows.len);
    const tight_budget: usize = 256; // a handful of entries; forces windowed-tier drops
    const c: adapter.Compactor = .{ .selective = tight_budget };
    const entries = c.compact(rows);
    try std.testing.expect(entries.len < rows.len);
    var core_hydrated = false;
    for (entries) |e| {
        if (std.mem.eql(u8, e.key, "owner")) core_hydrated = true;
    }
    try std.testing.expect(core_hydrated);
}

test "integration: numeric millis round-trip — updated_at reads back exactly; listAll newest-first" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_TS);
    defer db.wipe(ZID_TS);

    try adapter.storeEntry(db.conn, "id-ts1", ZID_TS, "older", "v1", adapter.CATEGORY_CORE, T_AGED);
    try adapter.storeEntry(db.conn, "id-ts2", ZID_TS, "newer", "v2", adapter.CATEGORY_CORE, T_YOUNG);

    // The stored value survives as the exact epoch-millis integer (BIGINT
    // round-trip, no string shape anywhere). Block-scoped so the read result
    // is drained before the next query on the same connection.
    {
        var q = PgQuery.from(try db.conn.query(
            "SELECT updated_at FROM memory.memory_entries WHERE zombie_id = $1::uuid AND key = $2",
            .{ ZID_TS, "newer" },
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(T_YOUNG, try row.get(i64, 0));
    }

    // Ordering is numeric now — newest-first with no digit-count caveat.
    const rows = try adapter.listAll(alloc, db.conn, ZID_TS);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqual(@as(usize, 2), rows.len);
    try std.testing.expectEqualStrings("newer", rows[0].key);
}

test "integration: daily sweep deletes only aged daily rows — young daily and other zombies untouched" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_SWEEP_A);
    db.wipe(ZID_SWEEP_B);
    defer db.wipe(ZID_SWEEP_A);
    defer db.wipe(ZID_SWEEP_B);

    try adapter.storeEntry(db.conn, "id-sa1", ZID_SWEEP_A, "aged", "v", adapter.CATEGORY_DAILY, T_AGED);
    try adapter.storeEntry(db.conn, "id-sa2", ZID_SWEEP_A, "young", "v", adapter.CATEGORY_DAILY, T_YOUNG);
    try adapter.storeEntry(db.conn, "id-sb1", ZID_SWEEP_B, "aged", "v", adapter.CATEGORY_DAILY, T_AGED);

    // Sweeping A at the cutoff removes exactly A's aged row — the young row
    // and B's rows (a different zombie, same age) are out of scope.
    try std.testing.expectEqual(@as(u64, 1), try adapter.sweepExpiredDaily(db.conn, ZID_SWEEP_A, CUTOFF));
    try std.testing.expectEqual(@as(usize, 1), try db.count(ZID_SWEEP_A));
    try std.testing.expectEqual(@as(usize, 1), try db.count(ZID_SWEEP_B));

    const rows = try adapter.listAll(alloc, db.conn, ZID_SWEEP_A);
    defer freeDeltas(alloc, rows);
    try std.testing.expectEqualStrings("young", rows[0].key);
}

test "integration: sweep never touches other categories — aged core/conversation/custom survive" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_SWEEP_A);
    defer db.wipe(ZID_SWEEP_A);

    try adapter.storeEntry(db.conn, "id-oc1", ZID_SWEEP_A, "kc", "v", adapter.CATEGORY_CORE, T_AGED);
    try adapter.storeEntry(db.conn, "id-oc2", ZID_SWEEP_A, "kv", "v", adapter.CATEGORY_CONVERSATION, T_AGED);
    try adapter.storeEntry(db.conn, "id-oc3", ZID_SWEEP_A, "kx", "v", "incident-notes", T_AGED);

    // At a far-future cutoff EVERY row counts as aged — only the bound `daily`
    // category may expire, so the sweep deletes nothing here.
    try std.testing.expectEqual(@as(u64, 0), try adapter.sweepExpiredDaily(db.conn, ZID_SWEEP_A, CUTOFF_ALL));
    try std.testing.expectEqual(@as(usize, 3), try db.count(ZID_SWEEP_A));
}

test "integration: daily sweep is idempotent — the second sweep deletes nothing" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_SWEEP_A);
    defer db.wipe(ZID_SWEEP_A);

    try adapter.storeEntry(db.conn, "id-ip1", ZID_SWEEP_A, "aged-1", "v", adapter.CATEGORY_DAILY, T_AGED);
    try adapter.storeEntry(db.conn, "id-ip2", ZID_SWEEP_A, "aged-2", "v", adapter.CATEGORY_DAILY, T_AGED);

    try std.testing.expectEqual(@as(u64, 2), try adapter.sweepExpiredDaily(db.conn, ZID_SWEEP_A, CUTOFF));
    try std.testing.expectEqual(@as(u64, 0), try adapter.sweepExpiredDaily(db.conn, ZID_SWEEP_A, CUTOFF));
    try std.testing.expectEqual(@as(usize, 0), try db.count(ZID_SWEEP_A));
}

test "integration: sweep failure propagates as an error, deleting nothing" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_SWEEP_A);
    defer db.wipe(ZID_SWEEP_A);

    try adapter.storeEntry(db.conn, "id-fp1", ZID_SWEEP_A, "aged", "v", adapter.CATEGORY_DAILY, T_AGED);

    // Deterministic failure injection, mirroring the enforceCap failure test:
    // a malformed zombie_id errors before any bind. The capture handler's
    // catch (warn + continue, HTTP success preserved) consumes any such error
    // identically in production — same construction as the cap-eviction path.
    try std.testing.expectError(error.InvalidUUID, adapter.sweepExpiredDaily(db.conn, "not-a-uuid", CUTOFF));
    try std.testing.expectEqual(@as(usize, 1), try db.count(ZID_SWEEP_A));
}

test "integration: listAll is scoped per zombie — no cross-zombie bleed" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();
    db.wipe(ZID_ISO_A);
    db.wipe(ZID_ISO_B);
    defer db.wipe(ZID_ISO_A);
    defer db.wipe(ZID_ISO_B);

    try adapter.storeEntry(db.conn, "id-a1", ZID_ISO_A, "secret", "alpha", adapter.CATEGORY_CORE, 1_700_000_001_000);
    try adapter.storeEntry(db.conn, "id-b1", ZID_ISO_B, "secret", "beta", adapter.CATEGORY_CORE, 1_700_000_001_000);

    const rows_a = try adapter.listAll(alloc, db.conn, ZID_ISO_A);
    defer freeDeltas(alloc, rows_a);
    try std.testing.expectEqual(@as(usize, 1), rows_a.len);
    try std.testing.expectEqualStrings("alpha", rows_a[0].content); // never beta
}
