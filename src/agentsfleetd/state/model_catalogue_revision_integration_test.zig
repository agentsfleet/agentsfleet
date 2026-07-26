//! Integration tier for §2 Dimension 2.2 — one generation governs both caches.
//!
//! Spec row: *"publish-after-commit and cross-replica fail-closed billing."*
//!
//! The two caches enforce the generation differently, and the difference is the
//! point:
//!
//!   - The **response cache** enforces it STRUCTURALLY, by carrying the revision
//!     in its key. A stale candidate is unreachable rather than dangerous — a
//!     request that has read N+1 never looks under the N key.
//!   - The **billing rate cache** cannot use that trick: it is keyed by
//!     `(provider, model_id)`, not by revision, because a price is looked up by
//!     identity and not by generation. So it reconciles explicitly, under its
//!     mutex, against the revision it observed.
//!
//! What this file proves against a real database is the half that a unit test
//! cannot: that the increment is atomic with the catalogue mutation, that two
//! concurrent mutations cannot share a generation, and that a reader either sees
//! both the change and its new generation or neither.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");
const common = @import("common");
const pg = @import("pg");

const base = @import("../db/test_fixtures.zig");
const revision = @import("model_catalogue_revision.zig");

/// Matches the lock-contention timeout used by the reference-transaction suite:
/// long enough not to misread a loaded machine, short enough not to stall.
const LOCK_TIMEOUT = "SET lock_timeout = '400ms'";
const LOCK_TIMEOUT_OFF = "SET lock_timeout = 0";

const TestDb = struct {
    pool: *pg.Pool,
    a: *pg.Conn,
    b: *pg.Conn,

    fn open(alloc: std.mem.Allocator) !?TestDb {
        if (common.env.testLiveValue("LIVE_DB") == null) return null;
        const ctx = (try base.openTestConn(alloc)) orelse return null;
        const second = ctx.pool.acquire() catch {
            ctx.pool.release(ctx.conn);
            ctx.pool.deinit();
            return null;
        };
        return .{ .pool = ctx.pool, .a = ctx.conn, .b = second };
    }

    fn close(self: TestDb) void {
        self.pool.release(self.b);
        self.pool.release(self.a);
        self.pool.deinit();
    }
};

/// Try to take the generation lock under a timeout. True when acquired (and
/// released again), false when someone else holds it.
fn tryLockRevision(conn: *pg.Conn) !bool {
    _ = try conn.exec(LOCK_TIMEOUT, .{});
    defer _ = conn.exec(LOCK_TIMEOUT_OFF, .{}) catch |err|
        std.log.warn("lock_timeout reset ignored: {s}", .{@errorName(err)});
    _ = try conn.exec("BEGIN", .{});
    const got = if (revision.lock(conn)) |_| true else |_| false;
    conn.rollback() catch |err| std.log.warn("contender rollback ignored: {s}", .{@errorName(err)});
    return got;
}

test "integration: test_catalogue_revision_governs_both_caches" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    // The singleton is seeded by the migration, so a read must succeed on a
    // freshly migrated database with no setup at all. A missing row would take
    // the catalogue offline (UZ-LIBRARY-004), so this is the migration's
    // contract with the read path.
    const start = try revision.read(db.a);
    try std.testing.expect(start >= 0);

    // A bump is visible to a LATER read and moves strictly forward.
    _ = try db.a.exec("BEGIN", .{});
    const locked = try revision.lock(db.a);
    try std.testing.expectEqual(start, locked);
    const bumped = try revision.bumpLocked(db.a, 1_745_884_800_000);
    _ = try db.a.exec("COMMIT", .{});

    try std.testing.expectEqual(start + 1, bumped);
    try std.testing.expectEqual(bumped, try revision.read(db.a));
}

test "integration: test_catalogue_revision_governs_both_caches: an uncommitted bump is invisible" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    const start = try revision.read(db.a);

    // Mutate and bump, but do NOT commit. This is the publish-after-commit
    // requirement: a reader must not observe the new generation until the
    // catalogue change it describes is durable, or it would cache a page under
    // a generation whose contents may still roll back.
    _ = try db.a.exec("BEGIN", .{});
    _ = try revision.lock(db.a);
    _ = try revision.bumpLocked(db.a, 1_745_884_800_000);

    // A different session still sees the old generation.
    try std.testing.expectEqual(start, try revision.read(db.b));

    // And after rollback it stays that way — the generation did not leak.
    db.a.rollback() catch |err| std.log.warn("rollback ignored: {s}", .{@errorName(err)});
    try std.testing.expectEqual(start, try revision.read(db.b));
    try std.testing.expectEqual(start, try revision.read(db.a));
}

test "integration: test_catalogue_revision_governs_both_caches: two mutations cannot share a generation" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    // Baseline: uncontended, the lock is available. Without this a timeout for
    // any unrelated reason would read as a successful exclusion below.
    try std.testing.expect(try tryLockRevision(db.b));

    _ = try db.a.exec("BEGIN", .{});
    _ = try revision.lock(db.a);

    // The second mutation cannot even READ the generation it would increment,
    // so it cannot compute the same next value. This is the whole reason the
    // mutation path locks and the read path does not.
    try std.testing.expect(!(try tryLockRevision(db.b)));

    const mine = try revision.bumpLocked(db.a, 1_745_884_800_000);
    _ = try db.a.exec("COMMIT", .{});

    // Released, and the next mutation starts from the generation the first
    // produced rather than from the one it read.
    try std.testing.expect(try tryLockRevision(db.b));
    try std.testing.expectEqual(mine, try revision.read(db.b));
}

test "integration: test_catalogue_revision_governs_both_caches: the hot-path read never blocks a writer" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    // A plain read is taken while a mutation holds the lock. It must succeed
    // immediately with the pre-mutation generation: catalogue reads are the hot
    // path, and serializing them behind an occasional admin write would trade a
    // real cost for no correctness gain.
    const start = try revision.read(db.a);

    _ = try db.a.exec("BEGIN", .{});
    _ = try revision.lock(db.a);

    _ = try db.b.exec(LOCK_TIMEOUT, .{});
    defer _ = db.b.exec(LOCK_TIMEOUT_OFF, .{}) catch |err|
        std.log.warn("lock_timeout reset ignored: {s}", .{@errorName(err)});
    // Would time out rather than return if `read` had taken FOR UPDATE.
    try std.testing.expectEqual(start, try revision.read(db.b));

    db.a.rollback() catch |err| std.log.warn("rollback ignored: {s}", .{@errorName(err)});
}
