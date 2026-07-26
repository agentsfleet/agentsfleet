//! Integration proof that the process-global rate cache's load/evict cycle is
//! leak-free, and that a load actually resolves a real catalogue row. Reads
//! `core.model_library`, so this needs a live DB; it skips gracefully when
//! TEST_DATABASE_URL / DATABASE_URL is unset.
//!
//! ## What the leak surface became
//!
//! The cache used to hold a whole-catalogue arena rebuilt by `populate`, so the
//! audit was "does a swap free the prior arena". There is no arena and no swap
//! now: entries are loaded one row at a time into `common.CacheTable`, and the
//! memory they own is the KEY — the `(provider, model)` strings, duped on insert
//! and freed by `RateKeyContext.evicted`.
//!
//! So the audit is the departure rule instead. Every entry that leaves the table
//! must be released exactly once, whatever removed it, and the table offers six
//! ways out. The soak below drives the two a live cache actually hits — bucket
//! overflow under distinct keys, and same-key refresh — by loading far more
//! distinct pairs than the table has slots. Under `testing.allocator`, an
//! eviction that forgot to free its key is a leak, and a double free is a crash.

const std = @import("std");
const testing = std.testing;
const clock = @import("common").clock;
const rss = @import("common").rss;
const base = @import("../db/test_fixtures.zig");
const model_rate_cache = @import("model_rate_cache.zig");
const revision_state = @import("model_catalogue_revision.zig");

// Suite-private (provider, model) + a uuidv7 uid so the seed never collides with
// another suite's core.model_library rows (version nibble 7 satisfies the uid CHECK).
const RC_UID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0abc01";
const RC_PROVIDER = "ratecache-probe";
/// A second private provider for the eviction soak, so its 3000 rows never
/// collide with the single-row tests above.
const RC_EVICT_PROVIDER = "ratecache-evict-probe";
/// First 24 chars of the eviction soak's uids; the loop index supplies the last
/// 12 hex digits. The '7' at offset 14 is the uuidv7 version nibble
/// `ck_model_library_uid_uuidv7` checks for.
const RC_EVICT_UID_PREFIX = "0195b4ba-8d3a-7f14-8abd-";
/// `BUCKET_COUNT * BUCKET_SIZE` in model_rate_cache.zig. Duplicated rather than
/// exported: the ceiling being a compile-time property of the TYPE is the claim,
/// and a test reading it from the module under test would assert nothing.
const RC_TABLE_CAPACITY: usize = 256 * 4;
const RC_MODEL = "rc-probe-model";
/// More distinct pairs than the table holds (1024 slots), so the run is
/// guaranteed to evict rather than merely fill. Every eviction is a key that
/// must be freed exactly once.
const RC_DISTINCT_PAIRS: usize = 3_000;
/// Re-loads of ONE already-resident pair. A same-key refresh displaces the old
/// entry just as surely as an eviction does, and it is the departure path the
/// primitive's own `put` used to report through a second channel — the one an
/// owner reading the return value leaked on every write.
const RC_REFRESH_CYCLES: usize = 50;

// Process-level RSS soak: the coarse (Bun-style) leak layer for the rate cache's
// PRODUCTION page_allocator backing — the key churn testing.allocator can't see
// (that in-process oracle is the test above). Warm to the allocator's plateau
// first so the baseline excludes one-time page mapping, then measure growth over
// the soak against a generous, coarse bound.
const RC_RSS_WARMUP_CYCLES: usize = 8; // prime the page_allocator plateau pre-baseline
const RC_RSS_SOAK_CYCLES: usize = 256; // load/evict rounds measured vs baseline
const RC_RSS_GROWTH_BOUND_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB — catches unbounded growth, not byte-exact

// Arbitrary seeded rates — the test asserts the row is cached, not its values,
// so these are named only to keep the seed self-documenting (and UFS-clean).
const RC_CAP_TOKENS: i32 = 256_000;
const RC_INPUT_NANOS: i64 = 1_000;
const RC_CACHED_NANOS: i64 = 100;
const RC_OUTPUT_NANOS: i64 = 2_000;

const RC_CLEANUP_SQL = "DELETE FROM core.model_library WHERE provider = $1";

// Seed the suite-private (provider, model) row both tests populate/swap
// against; caller owns the matching `defer _ = conn.exec(DELETE...) catch {}`
// (kept inline, not a shared helper — ZLint's suppressed-errors rule allows a
// swallowed `catch {}` as a defer's direct body, not inside a plain fn).
fn seedRateRow(conn: anytype) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ RC_UID, RC_MODEL, RC_PROVIDER, RC_CAP_TOKENS, RC_INPUT_NANOS, RC_CACHED_NANOS, RC_OUTPUT_NANOS, now });
}

/// Load `count` distinct synthetic pairs. They are absent from the catalogue, so
/// each costs one statement and caches nothing — which is exactly the *negative*
/// path, and it must not allocate. The resident-key churn comes from `refresh`.
fn loadAbsentPairs(conn: anytype, revision: i64, count: usize) !void {
    var buf: [64]u8 = undefined;
    var i: usize = 0;
    while (i < count) : (i += 1) {
        const model = try std.fmt.bufPrint(&buf, "rc-absent-{d}", .{i});
        try testing.expect((try model_rate_cache.rateAtRevision(conn, revision, RC_PROVIDER, model)) == null);
    }
}

test "integration(model_rate_cache): load, refresh and evict cycles are leak-free under testing.allocator" {
    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    try seedRateRow(conn);
    defer _ = conn.exec(RC_CLEANUP_SQL, .{RC_PROVIDER}) catch {};

    // Swap the backing to testing.allocator so every key dupe and every
    // `evicted` free is audited. The swap clears first, so nothing a prior test
    // left behind is freed by the wrong allocator.
    const prev = model_rate_cache.setBackingAllocatorForTest(testing.allocator);
    defer _ = model_rate_cache.setBackingAllocatorForTest(prev);
    defer model_rate_cache.clear(); // release the last testing.allocator-owned keys

    const revision = try revision_state.read(conn);

    // A real row loads and caches.
    const first = (try model_rate_cache.rateAtRevision(conn, revision, RC_PROVIDER, RC_MODEL)) orelse
        return error.TestUnexpectedResult;
    try testing.expectEqual(RC_INPUT_NANOS, first.input_nanos_per_mtok);
    try testing.expect(model_rate_cache.count() >= 1);

    // Same-key refresh: each round displaces the previous entry, whose key must
    // be freed exactly once. A leak here is the most common write a cache makes.
    var r: usize = 0;
    while (r < RC_REFRESH_CYCLES) : (r += 1) {
        // A revision strictly ahead of the entry's forces the reload rather than
        // letting the peek satisfy it — otherwise this loop would test nothing.
        _ = try model_rate_cache.rateAtRevision(conn, revision + 1 + @as(i64, @intCast(r)), RC_PROVIDER, RC_MODEL);
    }

    // Absent pairs: the negative path caches nothing, so the live count must not
    // have grown with them. This is what proves a miss is not quietly admitting
    // an entry per unknown model — an unbounded-growth shape a capacity ceiling
    // would hide and only a count assertion catches.
    const before_absent = model_rate_cache.count();
    try loadAbsentPairs(conn, revision, 64);
    try testing.expectEqual(before_absent, model_rate_cache.count());

    // The seeded row still resolves after all that churn.
    try testing.expect((try model_rate_cache.rateAtRevision(conn, revision, RC_PROVIDER, RC_MODEL)) != null);
}

test "integration(model_rate_cache): eviction under capacity pressure releases every key" {
    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    const prev = model_rate_cache.setBackingAllocatorForTest(testing.allocator);
    defer _ = model_rate_cache.setBackingAllocatorForTest(prev);
    defer model_rate_cache.clear();

    const revision = try revision_state.read(conn);

    // Seed ONE real row per iteration under a distinct (provider, model), so
    // every load admits an entry and the table is driven well past its 1024
    // slots. Whatever LRU pushes out must have its key freed by `evicted`; at
    // this volume a single missed free is a testing.allocator leak.
    var model_buf: [64]u8 = undefined;
    var uid_buf: [36]u8 = undefined;
    var i: usize = 0;
    while (i < RC_DISTINCT_PAIRS) : (i += 1) {
        const model = try std.fmt.bufPrint(&model_buf, "rc-evict-{d}", .{i});
        // Built by hand, not `gen_random_uuid()`: that yields a v4 uuid and
        // `ck_model_library_uid_uuidv7` requires a '7' in the version nibble, so
        // every insert would have failed the CHECK rather than the test.
        const uid = try std.fmt.bufPrint(&uid_buf, "{s}{x:0>12}", .{ RC_EVICT_UID_PREFIX, i });
        const now = clock.nowMillis();
        _ = try conn.exec(
            \\INSERT INTO core.model_library
            \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
            \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
            \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
            \\ON CONFLICT (provider, model_id) DO NOTHING
        , .{ uid, model, RC_EVICT_PROVIDER, RC_CAP_TOKENS, RC_INPUT_NANOS, RC_CACHED_NANOS, RC_OUTPUT_NANOS, now });
        _ = try model_rate_cache.rateAtRevision(conn, revision, RC_EVICT_PROVIDER, model);
    }
    defer _ = conn.exec(RC_CLEANUP_SQL, .{RC_EVICT_PROVIDER}) catch {};

    // The ceiling is structural: capacity is a compile-time slot count, so no
    // number of distinct loads can push the live set past it.
    try testing.expect(model_rate_cache.count() <= RC_TABLE_CAPACITY);
}

test "integration(model_rate_cache): RSS growth over the load/evict soak stays bounded (production page_allocator)" {
    // Skip early on a platform without an RSS reader — the probe can't run, and
    // that is a skip, never a failure (rss.zig returns null off Linux/macOS).
    if (rss.currentBytes() == null) return error.SkipZigTest;

    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Seed the same suite-private row so each populate does real row-parse +
    // arena work (not a no-op empty build).
    try seedRateRow(conn);
    defer _ = conn.exec(RC_CLEANUP_SQL, .{RC_PROVIDER}) catch {};

    // DELIBERATELY do NOT call setBackingAllocatorForTest: the probe's whole
    // point is the production page_allocator layer testing.allocator can't see.
    model_rate_cache.clear(); // drop whatever a prior test left resident
    defer model_rate_cache.clear();

    const revision = try revision_state.read(conn);

    // Warm to the allocator plateau BEFORE reading the baseline. Each round
    // forces a reload by demanding a generation ahead of the cached one.
    var w: usize = 0;
    while (w < RC_RSS_WARMUP_CYCLES) : (w += 1) {
        _ = try model_rate_cache.rateAtRevision(conn, revision + 1 + @as(i64, @intCast(w)), RC_PROVIDER, RC_MODEL);
    }

    const baseline = rss.currentBytes() orelse return error.SkipZigTest;
    var i: usize = 0;
    while (i < RC_RSS_SOAK_CYCLES) : (i += 1) {
        _ = try model_rate_cache.rateAtRevision(conn, revision + 1 + @as(i64, @intCast(i)), RC_PROVIDER, RC_MODEL);
    }
    const after = rss.currentBytes() orelse return error.SkipZigTest;

    // Saturating: RSS can dip below baseline as the OS recycles freed pages.
    const growth = after -| baseline;
    try testing.expect(growth < RC_RSS_GROWTH_BOUND_BYTES);
    // The soak actually resolved the row each round rather than short-circuiting.
    try testing.expect((try model_rate_cache.rateAtRevision(conn, revision, RC_PROVIDER, RC_MODEL)) != null);
}
