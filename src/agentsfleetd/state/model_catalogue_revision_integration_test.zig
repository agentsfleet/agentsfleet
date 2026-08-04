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
//!   - The **billing rate cache** is keyed by `(provider, model_id)` — a price is
//!     looked up by identity, not by generation — and carries the generation in
//!     its VALUE. A charge accepts a cached entry only at the generation it
//!     observed or later, and otherwise reloads the row. (An earlier draft had it
//!     reconciling the whole cache under its mutex, on the reasoning that an
//!     identity-keyed cache "cannot use this trick". Storing the generation
//!     beside the rate IS that trick, so the protocol is gone — see §Discovery.)
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
const model_rate_cache = @import("model_rate_cache.zig");

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

// ── The billing half: cross-replica fail-closed pricing ─────────────────────

/// Suite-private catalogue identity. Its 15th character is the uuidv7 version
/// nibble `ck_model_library_id_uuidv7` requires.
const BILL_UID = "0195b4ba-8d3a-7f15-8abe-2b3e1e0abd01";
const BILL_PROVIDER = "revision-billing-probe";
const BILL_MODEL = "rb-probe-model";
const BILL_CAP: i32 = 128_000;
const BILL_PRICE_BEFORE: i64 = 1_000;
const BILL_PRICE_AFTER: i64 = 7_777;

fn seedBillingRow(conn: *pg.Conn, input_nanos: i64) !void {
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (id, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, 0, 0, $6, $6)
        \\ON CONFLICT (provider, model_id) DO UPDATE SET
        \\   input_nanos_per_mtok = EXCLUDED.input_nanos_per_mtok,
        \\   updated_at = EXCLUDED.updated_at
    , .{ BILL_UID, BILL_MODEL, BILL_PROVIDER, BILL_CAP, input_nanos, @as(i64, 1_745_884_800_000) });
}

test "integration: test_catalogue_revision_governs_both_caches: a replica that missed the mutation cannot bill the old rate" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    try seedBillingRow(db.a, BILL_PRICE_BEFORE);
    defer _ = db.a.exec("DELETE FROM core.model_library WHERE provider = $1", .{BILL_PROVIDER}) catch {};
    model_rate_cache.clear();
    defer model_rate_cache.clear();

    // This replica prices a slice and caches the rate at the generation it saw.
    const before = try revision.read(db.a);
    const priced = (try model_rate_cache.rateAtRevision(db.a, before, BILL_PROVIDER, BILL_MODEL)) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqual(BILL_PRICE_BEFORE, priced.input_nanos_per_mtok);

    // ANOTHER replica raises the price and bumps the generation, in one
    // transaction — exactly what the admin handler does. This process is not
    // told: no `clear()` runs here, which is the whole point. A sibling replica
    // has no channel to invalidate this one's cache.
    _ = try db.b.exec("BEGIN", .{});
    _ = try revision.lock(db.b);
    try seedBillingRow(db.b, BILL_PRICE_AFTER);
    const after = try revision.bumpLocked(db.b, 1_745_884_800_001);
    _ = try db.b.exec("COMMIT", .{});
    try std.testing.expect(after > before);

    // The stale entry IS still resident — asserted, because without this the
    // test below would pass just as well on an empty cache, and would then be
    // proving nothing about staleness at all.
    const resident = model_rate_cache.cachedRate(BILL_PROVIDER, BILL_MODEL) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqual(BILL_PRICE_BEFORE, resident.input_nanos_per_mtok);

    // A charge that has observed the NEW generation refuses the resident entry
    // and reloads. This is the fail-closed guarantee: the slice is priced at the
    // catalogue's current rate even though this replica's cache never heard
    // about the change.
    const repriced = (try model_rate_cache.rateAtRevision(db.a, after, BILL_PROVIDER, BILL_MODEL)) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqual(BILL_PRICE_AFTER, repriced.input_nanos_per_mtok);
}

test "integration: test_catalogue_revision_governs_both_caches: an uncommitted price change is never billed" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    try seedBillingRow(db.a, BILL_PRICE_BEFORE);
    defer _ = db.a.exec("DELETE FROM core.model_library WHERE provider = $1", .{BILL_PROVIDER}) catch {};
    model_rate_cache.clear();
    defer model_rate_cache.clear();

    const before = try revision.read(db.a);

    // An admin mutation in flight: the row is changed and the generation bumped,
    // but nothing is committed.
    _ = try db.b.exec("BEGIN", .{});
    _ = try revision.lock(db.b);
    try seedBillingRow(db.b, BILL_PRICE_AFTER);
    _ = try revision.bumpLocked(db.b, 1_745_884_800_002);

    // A concurrent charge prices at the committed generation and gets the
    // committed rate — never the uncommitted one, which may still roll back.
    const priced = (try model_rate_cache.rateAtRevision(db.a, before, BILL_PROVIDER, BILL_MODEL)) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqual(BILL_PRICE_BEFORE, priced.input_nanos_per_mtok);

    db.b.rollback() catch |err| std.log.warn("rollback ignored: {s}", .{@errorName(err)});

    // And after the rollback the generation never moved, so nothing cached
    // during the window has to be discarded.
    try std.testing.expectEqual(before, try revision.read(db.a));
    const after_rollback = (try model_rate_cache.rateAtRevision(db.a, before, BILL_PROVIDER, BILL_MODEL)) orelse
        return error.TestUnexpectedResult;
    try std.testing.expectEqual(BILL_PRICE_BEFORE, after_rollback.input_nanos_per_mtok);
}

test "integration: test_catalogue_revision_governs_both_caches: an absent model is null, not a stale rate" {
    const alloc = std.testing.allocator;
    const db = (try TestDb.open(alloc)) orelse return error.SkipZigTest;
    defer db.close();

    try seedBillingRow(db.a, BILL_PRICE_BEFORE);
    model_rate_cache.clear();
    defer model_rate_cache.clear();

    const before = try revision.read(db.a);
    _ = try model_rate_cache.rateAtRevision(db.a, before, BILL_PROVIDER, BILL_MODEL);

    // Delete the row and bump, as a DELETE /v1/admin/models does.
    _ = try db.b.exec("BEGIN", .{});
    _ = try revision.lock(db.b);
    _ = try db.b.exec("DELETE FROM core.model_library WHERE provider = $1", .{BILL_PROVIDER});
    const after = try revision.bumpLocked(db.b, 1_745_884_800_003);
    _ = try db.b.exec("COMMIT", .{});

    // The cache still holds the deleted model's rate, and the generation check
    // is what stops it being billed. `null` here is a DATABASE answer — which is
    // what makes it safe for callers to treat as "not catalogued".
    try std.testing.expect(model_rate_cache.cachedRate(BILL_PROVIDER, BILL_MODEL) != null);
    try std.testing.expect((try model_rate_cache.rateAtRevision(db.a, after, BILL_PROVIDER, BILL_MODEL)) == null);
}
