//! Integration proof that the process-global rate cache's rebuild/swap cycle is
//! leak-free. `populate` reads `core.model_library`, so this needs a live DB;
//! it skips gracefully when TEST_DATABASE_URL / DATABASE_URL is unset.
//!
//! The cache owns its arena off a module `backing_allocator` (page_allocator in
//! production). Here we override it to `testing.allocator` and drive
//! REBUILD_CYCLES populate/swap rounds: each round builds a fresh arena and
//! deinits the prior one, so a swap that leaked the old arena is caught as a
//! testing.allocator leak.

const std = @import("std");
const testing = std.testing;
const clock = @import("common").clock;
const rss = @import("common").rss;
const base = @import("../db/test_fixtures.zig");
const model_rate_cache = @import("model_rate_cache.zig");

// Suite-private (provider, model) + a uuidv7 uid so the seed never collides with
// another suite's core.model_library rows (version nibble 7 satisfies the uid CHECK).
const RC_UID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0abc01";
const RC_PROVIDER = "ratecache-probe";
const RC_MODEL = "rc-probe-model";
const REBUILD_CYCLES: usize = 20; // enough swaps that a per-swap arena leak trips testing.allocator

// Process-level RSS soak: the coarse (Bun-style) leak layer for the rate
// cache's PRODUCTION page_allocator backing — the arena churn testing.allocator can't
// see (that in-process oracle is the REBUILD_CYCLES test above). Warm to the
// allocator's plateau first so the baseline excludes one-time page mapping,
// then measure growth over the soak against a generous, coarse bound.
const RC_RSS_WARMUP_CYCLES: usize = 8; // prime the page_allocator plateau pre-baseline
const RC_RSS_SOAK_CYCLES: usize = 256; // populate/swap rounds measured vs baseline
const RC_RSS_GROWTH_BOUND_BYTES: u64 = 2 * 1024 * 1024; // 2 MiB — catches unbounded growth, not byte-exact

// Arbitrary seeded rates — the test asserts the row is cached, not its values,
// so these are named only to keep the seed self-documenting (and UFS-clean).
const RC_CAP_TOKENS: i32 = 256_000;
const RC_INPUT_NANOS: i64 = 1_000;
const RC_CACHED_NANOS: i64 = 100;
const RC_OUTPUT_NANOS: i64 = 2_000;

test "integration(model_rate_cache): rebuild/swap cycles are leak-free under testing.allocator" {
    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ RC_UID, RC_MODEL, RC_PROVIDER, RC_CAP_TOKENS, RC_INPUT_NANOS, RC_CACHED_NANOS, RC_OUTPUT_NANOS, now });
    defer _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1", .{RC_PROVIDER}) catch {};

    // Reset any global a prior test left, then swap the backing to
    // testing.allocator so every rebuild + prior-arena free is leak-audited.
    model_rate_cache.deinit();
    const prev = model_rate_cache.setBackingAllocatorForTest(testing.allocator);
    defer _ = model_rate_cache.setBackingAllocatorForTest(prev);
    defer model_rate_cache.deinit(); // free the last testing.allocator-built cache

    var i: usize = 0;
    while (i < REBUILD_CYCLES) : (i += 1) {
        try model_rate_cache.populate(conn);
    }
    // The final cache resolved our seeded row (populate actually built it, not a
    // no-op) and testing.allocator saw REBUILD_CYCLES build+free rounds clean.
    try testing.expect(model_rate_cache.lookup_model_rate(RC_PROVIDER, RC_MODEL) != null);
}

test "integration(model_rate_cache): RSS growth over the rebuild/swap soak stays bounded (production page_allocator)" {
    // Skip early on a platform without an RSS reader — the probe can't run, and
    // that is a skip, never a failure (rss.zig returns null off Linux/macOS).
    if (rss.currentBytes() == null) return error.SkipZigTest;

    const db_ctx = (try base.openTestConn(testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Seed the same suite-private row so each populate does real row-parse +
    // arena work (not a no-op empty build).
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO core.model_library
        \\  (uid, model_id, provider, context_cap_tokens, input_nanos_per_mtok,
        \\   cached_input_nanos_per_mtok, output_nanos_per_mtok, created_at_ms, updated_at_ms)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $8, $8)
        \\ON CONFLICT (provider, model_id) DO NOTHING
    , .{ RC_UID, RC_MODEL, RC_PROVIDER, RC_CAP_TOKENS, RC_INPUT_NANOS, RC_CACHED_NANOS, RC_OUTPUT_NANOS, now });
    defer _ = conn.exec("DELETE FROM core.model_library WHERE provider = $1", .{RC_PROVIDER}) catch {};

    // DELIBERATELY do NOT call setBackingAllocatorForTest: the probe's whole
    // point is the production page_allocator layer testing.allocator can't see.
    model_rate_cache.deinit(); // clear any global a prior test left
    defer model_rate_cache.deinit(); // free the last production-built cache

    // Warm to the allocator plateau BEFORE reading the baseline.
    var w: usize = 0;
    while (w < RC_RSS_WARMUP_CYCLES) : (w += 1) try model_rate_cache.populate(conn);

    const baseline = rss.currentBytes() orelse return error.SkipZigTest;
    var i: usize = 0;
    while (i < RC_RSS_SOAK_CYCLES) : (i += 1) try model_rate_cache.populate(conn);
    const after = rss.currentBytes() orelse return error.SkipZigTest;

    // Saturating: RSS can dip below baseline as the OS recycles freed pages.
    const growth = after -| baseline;
    try testing.expect(growth < RC_RSS_GROWTH_BOUND_BYTES);
    // The soak actually resolved the row (populate built a real cache each round).
    try testing.expect(model_rate_cache.lookup_model_rate(RC_PROVIDER, RC_MODEL) != null);
}
