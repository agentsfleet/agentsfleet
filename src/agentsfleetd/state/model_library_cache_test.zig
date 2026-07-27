//! Unit tier for §2 Dimension 2.2 — the catalogue response cache.
//!
//! Spec row: *"byte accounting per §2, 256-entry and 8 MiB ceilings, true LRU
//! eviction order, 60-second monotonic TTL, over-budget bypass."* Three of those
//! five no longer exist, and the spec is amended to match rather than these tests
//! being written to a claim the code does not make:
//!
//!   - **Eviction is by bucket, not global LRU.** A read does not promote, so the
//!     entry ceiling is asserted as "never more than `MAX_ENTRIES`, with
//!     near-total retention below capacity" rather than by naming a victim.
//!   - **The entry ceiling is structural.** Capacity IS the slot count, so the
//!     assertion is about the type's geometry, not a counter's discipline.
//!   - **There is no TTL, no byte ceiling, and no bypass.** Freshness comes from
//!     the revision in the KEY — a superseded page is unreachable, not stale — and
//!     the bound is the slot count. A `LibraryRow` is 188 bytes at the median of
//!     the shipped fixture ids, so 256 full pages is ≈2.3 MiB and the old 8 MiB
//!     byte total could never fire. See the module header.
//!
//! Nothing here reads or injects a clock, because the cache has no clock to
//! control.

const std = @import("std");

const cache_mod = @import("model_library_cache.zig");

const testing = std.testing;

/// A key standing for one set of canonical selectors at one generation. The
/// production digest is an HMAC under a process-random key; only distinctness
/// and fixed width matter to the cache, which is all this reproduces.
fn keyOf(revision: u64, seed: u64) cache_mod.Key {
    // SAFETY: `final` below writes every byte of `digest` before the key is
    // returned, so no caller can observe it uninitialized.
    var key: cache_mod.Key = .{ .revision = revision, .digest = undefined };
    var h = std.crypto.hash.sha2.Sha256.init(.{});
    h.update(std.mem.asBytes(&seed));
    h.final(&key.digest);
    return key;
}

// Every fetch below copies into `testing.allocator` — the caller's allocator,
// not the cache's. Freeing it here is what proves the copy is genuinely the
// caller's to own: if `fetch` ever went back to duping into the cache's own
// allocator, these frees would be cross-allocator and the leak check would fail.
fn expectHit(c: *cache_mod.Cache, key: cache_mod.Key, want: []const u8) !void {
    const got = (try c.fetch(testing.allocator, key)) orelse return error.ExpectedHit;
    defer testing.allocator.free(got);
    try testing.expectEqualStrings(want, got);
}

fn expectMiss(c: *cache_mod.Cache, key: cache_mod.Key) !void {
    const got = try c.fetch(testing.allocator, key);
    if (got) |g| {
        testing.allocator.free(g);
        return error.ExpectedMiss;
    }
}

test "a stored page is returned, and an absent one reads as a miss" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    try expectMiss(&c, keyOf(1, 1));
    try c.put(keyOf(1, 1), "payload-a");
    try expectHit(&c, keyOf(1, 1), "payload-a");
    try testing.expectEqual(@as(usize, 1), c.count());
}

test "a key never returns another key's page" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // The one failure a cache cannot recover from. Distinct seeds, so distinct
    // digests, and the pages must not cross even under bucket collision.
    var seed: u64 = 0;
    while (seed < 64) : (seed += 1) {
        var buf: [32]u8 = undefined;
        const body = try std.fmt.bufPrint(&buf, "page-{d}", .{seed});
        try c.put(keyOf(7, seed), body);
    }
    seed = 0;
    while (seed < 64) : (seed += 1) {
        var buf: [32]u8 = undefined;
        const want = try std.fmt.bufPrint(&buf, "page-{d}", .{seed});
        // A page may have been evicted — that is a cache's prerogative — but
        // whatever answers MUST be this key's own.
        if (try c.fetch(testing.allocator, keyOf(7, seed))) |got| {
            defer testing.allocator.free(got);
            try testing.expectEqualStrings(want, got);
        }
    }
}

test "capacity is structural and never exceeded" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Far more distinct keys than slots. The ceiling is the geometry, so this
    // cannot be exceeded by any sequence of writes.
    var seed: u64 = 0;
    while (seed < cache_mod.MAX_ENTRIES * 4) : (seed += 1) {
        try c.put(keyOf(1, seed), "page");
        try testing.expect(c.count() <= cache_mod.MAX_ENTRIES);
    }
}

test "retention is near-total below capacity" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // At half load the bucket distribution should keep nearly everything. Not
    // "everything": four ways per bucket means an unlucky digest cluster may
    // still evict, and pinning exact retention would pin the hash function.
    const written: u64 = cache_mod.MAX_ENTRIES / 2;
    var seed: u64 = 0;
    while (seed < written) : (seed += 1) try c.put(keyOf(1, seed), "page");

    var hits: usize = 0;
    seed = 0;
    while (seed < written) : (seed += 1) {
        if (try c.fetch(testing.allocator, keyOf(1, seed))) |got| {
            testing.allocator.free(got);
            hits += 1;
        }
    }
    try testing.expect(hits * 10 >= written * 9); // ≥90% retained
}

test "the revision in the key isolates generations" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    try c.put(keyOf(1, 1), "page-at-rev-1");
    // Same selectors, next generation: a DIFFERENT key, so the older page is
    // unreachable rather than stale. This is what replaces a freshness deadline.
    try expectMiss(&c, keyOf(2, 1));

    try c.put(keyOf(2, 1), "page-at-rev-2");
    try expectHit(&c, keyOf(2, 1), "page-at-rev-2");
    // The rev-1 page is still resident and still correct for anyone asking at
    // rev 1 — it ages out under bucket pressure, and nothing serves it to a
    // caller who has observed rev 2.
    try expectHit(&c, keyOf(1, 1), "page-at-rev-1");
}

test "a re-put of the same key is answered by the newer page" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Two concurrent misses can both build and both admit. The table appends
    // rather than replacing, so both are resident and the newest must answer.
    try c.put(keyOf(1, 1), "first-build");
    try c.put(keyOf(1, 1), "second-build");
    try expectHit(&c, keyOf(1, 1), "second-build");
}

test "eviction frees the displaced payload" {
    // `testing.allocator` is the instrument: a departure path that skipped
    // `Context.evicted` would leak here, and one that called it twice would
    // double-free. Neither is visible from the outside any other way.
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    var seed: u64 = 0;
    while (seed < cache_mod.MAX_ENTRIES * 4) : (seed += 1) {
        try c.put(keyOf(1, seed), "a payload long enough to notice if it leaks");
    }
    // Every page displaced along the way was freed by the hook; the deferred
    // deinit frees what is still resident.
}

test "teardown frees every resident payload" {
    var c = cache_mod.Cache.init(testing.allocator);

    var seed: u64 = 0;
    while (seed < 16) : (seed += 1) try c.put(keyOf(1, seed), "resident-at-teardown");
    try testing.expect(c.count() > 0);

    c.deinit(); // leak-checked by testing.allocator
}
