//! Unit tier for §2 Dimension 2.2 — response-cache accounting and eviction.
//!
//! Spec row: *"byte accounting per §2, 256-entry and 8 MiB ceilings, true LRU
//! eviction order, 60-second monotonic TTL, over-budget bypass."*
//!
//! Two of those changed shape when the storage became `common.CacheTable`, and
//! the spec is amended to match rather than the tests being written to a claim
//! the code no longer makes:
//!
//!   - **Eviction is least-recently-used within a bucket, not globally.** So
//!     the entry ceiling is asserted as "never more than 256, and near-total
//!     retention below capacity" instead of a naming of the exact victim.
//!   - **The entry ceiling is structural.** Capacity IS the slot count, so the
//!     test asserts the type's geometry rather than a counter's discipline.
//!
//! Time is injected rather than read from a clock. A TTL test that sleeps is
//! either slow or flaky, and a 60-second bound cannot be waited out at all — so
//! `now_ms` is a parameter and the tests step it directly. That also lets the
//! monotonic requirement be asserted honestly: the cache never consults a wall
//! clock, so there is no clock for a test to have to control.

const std = @import("std");

const cache_mod = @import("model_library_cache.zig");

const testing = std.testing;

/// Any monotonic origin will do; a non-zero one is used deliberately so a bug
/// that treats a zero timestamp as "unset" cannot pass.
const ORIGIN_MS: i64 = 1_000_000;

/// A clock reading far past any deadline this cache issues. Expiry must stay
/// expiry however long a process has been running — it must not wrap, and it
/// must not lapse back into freshness.
const LONG_AFTER_DEADLINE_MS: i64 = cache_mod.TTL_MS * 1_000;

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

fn putOk(c: *cache_mod.Cache, key: cache_mod.Key, value: []const u8, now_ms: i64) !void {
    try testing.expect(try c.put(key, value, now_ms));
}

fn expectHit(c: *cache_mod.Cache, key: cache_mod.Key, want: []const u8, now_ms: i64) !void {
    const got = (try c.fetch(key, now_ms)) orelse return error.ExpectedHit;
    defer testing.allocator.free(got);
    try testing.expectEqualStrings(want, got);
}

fn expectMiss(c: *cache_mod.Cache, key: cache_mod.Key, now_ms: i64) !void {
    const got = try c.fetch(key, now_ms);
    if (got) |g| {
        testing.allocator.free(g);
        return error.ExpectedMiss;
    }
}

test "test_response_cache_accounting_and_lru" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Empty means zero — not "roughly zero".
    try testing.expectEqual(@as(usize, 0), c.byteLen());
    try testing.expectEqual(@as(usize, 0), c.count(ORIGIN_MS));

    const k = keyOf(1, 0);
    try putOk(&c, k, "payload-a", ORIGIN_MS);
    try expectHit(&c, k, "payload-a", ORIGIN_MS);

    // Accounting is exactly the payload bytes — the sum the ceiling is enforced
    // against. Keys are fixed-size and stored inline, so unlike the predecessor
    // no key byte competes with a payload byte for the budget.
    try testing.expectEqual(@as(usize, "payload-a".len), c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count(ORIGIN_MS));

    // Refreshing a key must not double-count it. This is the write a 60-second
    // cache makes most often, and it is the path where an un-hooked overwrite
    // would leak both the bytes and the tally.
    try putOk(&c, k, "payload-a", ORIGIN_MS);
    try testing.expectEqual(@as(usize, "payload-a".len), c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count(ORIGIN_MS));

    // A longer payload under the same key re-bases the tally rather than adding.
    try putOk(&c, k, "payload-a-but-longer", ORIGIN_MS);
    try testing.expectEqual(@as(usize, "payload-a-but-longer".len), c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count(ORIGIN_MS));
}

test "test_response_cache_accounting_and_lru: a key never returns another key's page" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // The one failure a cache cannot recover from. Distinct digests must never
    // alias, including for keys that land in the same bucket.
    try putOk(&c, keyOf(1, 1), "page-one", ORIGIN_MS);
    try putOk(&c, keyOf(1, 2), "page-two", ORIGIN_MS);

    try expectHit(&c, keyOf(1, 1), "page-one", ORIGIN_MS);
    try expectHit(&c, keyOf(1, 2), "page-two", ORIGIN_MS);
    try expectMiss(&c, keyOf(1, 3), ORIGIN_MS);
}

test "test_response_cache_accounting_and_lru: capacity is structural and never exceeded" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Four times the capacity, so every bucket is pushed well past its ways.
    var i: u64 = 0;
    while (i < cache_mod.MAX_ENTRIES * 4) : (i += 1) {
        try putOk(&c, keyOf(1, i), "v", ORIGIN_MS);
        // The bound that matters holds on every single insert, not just at rest.
        try testing.expect(c.count(ORIGIN_MS) <= cache_mod.MAX_ENTRIES);
    }
    try testing.expectEqual(@as(usize, 256), cache_mod.MAX_ENTRIES);
}

test "test_response_cache_accounting_and_lru: retention is near-total below capacity" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // At half load a four-way bucket absorbs essentially every collision. This
    // is what replaces a global-LRU victim assertion: the promise is not which
    // entry leaves, it is that ordinary load does not thrash. Each loss costs
    // one catalogue rebuild, never a wrong answer.
    const population: u64 = cache_mod.MAX_ENTRIES / 2;
    var i: u64 = 0;
    while (i < population) : (i += 1) try putOk(&c, keyOf(1, i), "v", ORIGIN_MS);

    var resident: usize = 0;
    i = 0;
    while (i < population) : (i += 1) {
        if (try c.fetch(keyOf(1, i), ORIGIN_MS)) |v| {
            testing.allocator.free(v);
            resident += 1;
        }
    }
    try testing.expect(resident > population * 9 / 10);
}

test "test_response_cache_accounting_and_lru: the TTL is 60s and never reads a clock" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    const k = keyOf(1, 0);
    try putOk(&c, k, "payload", ORIGIN_MS);

    // One millisecond before the bound is still fresh.
    try expectHit(&c, k, "payload", ORIGIN_MS + cache_mod.TTL_MS - 1);
    // Expiry is a deadline, not a grace period.
    try expectMiss(&c, k, ORIGIN_MS + cache_mod.TTL_MS);
    // And it stays expired however far the caller's clock has moved on.
    try expectMiss(&c, k, ORIGIN_MS + LONG_AFTER_DEADLINE_MS);

    try testing.expectEqual(@as(i64, 60 * std.time.ms_per_s), cache_mod.TTL_MS);
}

test "test_response_cache_accounting_and_lru: an expired entry is reclaimed, not merely hidden" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Expiry is lazy: a dead entry keeps its slot and its bytes until something
    // reaches it. What must not happen is that its memory is held forever, so
    // pressure at the byte ceiling reclaims it.
    const chunk = cache_mod.MAX_BYTES / 2;
    const value = try testing.allocator.alloc(u8, chunk);
    defer testing.allocator.free(value);
    @memset(value, 'y');

    try putOk(&c, keyOf(1, 1), value, ORIGIN_MS);
    try putOk(&c, keyOf(1, 2), value, ORIGIN_MS);
    try testing.expectEqual(cache_mod.MAX_BYTES, c.byteLen());

    // Both are dead by now. Admitting a third would cross the ceiling on the
    // tally alone, so the dead pair is swept and the insert succeeds.
    const later = ORIGIN_MS + cache_mod.TTL_MS;
    try putOk(&c, keyOf(1, 3), value, later);
    try testing.expectEqual(chunk, c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count(later));
}

test "test_response_cache_accounting_and_lru: an oversized entry is bypassed, not an eviction cascade" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    const keep = keyOf(1, 0);
    try putOk(&c, keep, "v", ORIGIN_MS);
    const before = c.byteLen();

    // One value larger than the whole budget. Admitting it would mean emptying
    // the cache to hold a single outlier — trading a working cache for one hit.
    // It is refused instead, and `put` reports false so the caller knows to
    // serve the response uncached rather than assume it was stored.
    const huge = try testing.allocator.alloc(u8, cache_mod.MAX_BYTES + 1);
    defer testing.allocator.free(huge);
    @memset(huge, 'x');
    try testing.expect(!(try c.put(keyOf(1, 9), huge, ORIGIN_MS)));

    // The bypass left the existing contents completely untouched.
    try testing.expectEqual(before, c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count(ORIGIN_MS));
    try expectHit(&c, keep, "v", ORIGIN_MS);
    try expectMiss(&c, keyOf(1, 9), ORIGIN_MS);
}

test "test_response_cache_accounting_and_lru: the byte ceiling binds before the entry count does" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Values big enough that the BYTE ceiling binds long before 256 entries do.
    const admits = 8;
    const chunk = cache_mod.MAX_BYTES / admits;
    const value = try testing.allocator.alloc(u8, chunk);
    defer testing.allocator.free(value);
    @memset(value, 'y');

    var i: u64 = 0;
    while (i < admits * 3) : (i += 1) {
        _ = try c.put(keyOf(1, i), value, ORIGIN_MS);
        // The invariant that matters on every single insert.
        try testing.expect(c.byteLen() <= cache_mod.MAX_BYTES);
        try testing.expect(c.count(ORIGIN_MS) <= cache_mod.MAX_ENTRIES);
    }

    // Exactly the budget's worth got in, and nothing live was evicted to make
    // room for the ones that did not — the surplus was bypassed.
    try testing.expectEqual(@as(usize, admits), c.count(ORIGIN_MS));
    try testing.expectEqual(cache_mod.MAX_BYTES, c.byteLen());
}

test "test_response_cache_accounting_and_lru: the revision in the key isolates generations" {
    var c = try cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // The same selectors at two revisions are two keys. This is what makes a
    // stale candidate unreachable rather than dangerous: a request that has read
    // revision 2 never looks under the revision-1 key, so publish ordering
    // between concurrent builders stops mattering.
    try putOk(&c, keyOf(1, 7), "page-at-rev-1", ORIGIN_MS);
    try putOk(&c, keyOf(2, 7), "page-at-rev-2", ORIGIN_MS);

    try expectHit(&c, keyOf(1, 7), "page-at-rev-1", ORIGIN_MS);
    try expectHit(&c, keyOf(2, 7), "page-at-rev-2", ORIGIN_MS);
    try testing.expectEqual(@as(usize, 2), c.count(ORIGIN_MS));
}

test "test_response_cache_accounting_and_lru: teardown frees every resident payload" {
    // No explicit assertion: `std.testing.allocator` reports the leak. The point
    // is that entries left resident at deinit are freed, which is the path a
    // process shutdown takes and the one no request-path test would cover.
    var c = try cache_mod.Cache.init(testing.allocator);
    var i: u64 = 0;
    while (i < 32) : (i += 1) try putOk(&c, keyOf(1, i), "resident-at-teardown", ORIGIN_MS);
    c.deinit();
}
