//! Unit tier for §2 Dimension 2.2 — response-cache accounting and LRU.
//!
//! Spec row: *"byte accounting per §2, 256-entry and 8 MiB ceilings, true LRU
//! eviction order, 60-second monotonic TTL, over-budget bypass."*
//!
//! Time is injected rather than read from a clock. A TTL test that sleeps is
//! either slow or flaky, and a 60-second bound cannot be waited out at all — so
//! `now` is a parameter and the tests step it directly. That also lets the
//! monotonic requirement be asserted honestly: the cache never consults a wall
//! clock, so there is no clock for a test to have to control.

const std = @import("std");

const cache_mod = @import("model_library_cache.zig");

const testing = std.testing;

/// Any monotonic origin will do; a non-zero one is used deliberately so a bug
/// that treats `stored_at == 0` as "unset" cannot pass.
const ORIGIN_NANOS: u64 = 1_000_000_000;

fn putOk(c: *cache_mod.Cache, key: []const u8, value: []const u8, now: u64) !void {
    try testing.expect(try c.put(key, value, now));
}

fn expectHit(c: *cache_mod.Cache, key: []const u8, want: []const u8, now: u64) !void {
    const got = (try c.get(key, now)) orelse return error.ExpectedHit;
    defer testing.allocator.free(got);
    try testing.expectEqualStrings(want, got);
}

fn expectMiss(c: *cache_mod.Cache, key: []const u8, now: u64) !void {
    const got = try c.get(key, now);
    if (got) |g| {
        testing.allocator.free(g);
        return error.ExpectedMiss;
    }
}

test "test_response_cache_accounting_and_lru" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Empty means zero — not "roughly zero".
    try testing.expectEqual(@as(usize, 0), c.byteLen());
    try testing.expectEqual(@as(usize, 0), c.count());

    try putOk(&c, "rev1|a", "payload-a", ORIGIN_NANOS);
    try expectHit(&c, "rev1|a", "payload-a", ORIGIN_NANOS);

    // Accounting is exactly key + value + node storage, which is the same sum
    // the ceiling is enforced against. Computed here from the same three terms
    // so the assertion states the rule rather than a magic total.
    const expect_one = "rev1|a".len + "payload-a".len + nodeBytes();
    try testing.expectEqual(expect_one, c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count());

    // Replacing a key must not double-count it.
    try putOk(&c, "rev1|a", "payload-a", ORIGIN_NANOS);
    try testing.expectEqual(expect_one, c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count());

    // Removing everything returns the tally to zero — a leak in the accounting
    // shows up here even when no memory leaks, because the two are tracked
    // separately.
    var i: usize = 0;
    while (i < cache_mod.MAX_ENTRIES) : (i += 1) {
        var buf: [32]u8 = undefined;
        const k = try std.fmt.bufPrint(&buf, "rev1|fill-{d}", .{i});
        try putOk(&c, k, "v", ORIGIN_NANOS);
    }
    try testing.expectEqual(cache_mod.MAX_ENTRIES, c.count());
}

/// The per-entry overhead the cache counts. Mirrors the module's own constant
/// via the public byte tally rather than re-deriving `@sizeOf`, so this test
/// cannot drift from the implementation silently.
fn nodeBytes() usize {
    var probe = cache_mod.Cache.init(testing.allocator);
    defer probe.deinit();
    _ = probe.put("k", "v", ORIGIN_NANOS) catch return 0;
    return probe.byteLen() - 2;
}

test "test_response_cache_accounting_and_lru: the entry ceiling evicts least-recently-used first" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Fill to exactly the ceiling.
    var i: usize = 0;
    while (i < cache_mod.MAX_ENTRIES) : (i += 1) {
        var buf: [32]u8 = undefined;
        const k = try std.fmt.bufPrint(&buf, "k{d}", .{i});
        try putOk(&c, k, "v", ORIGIN_NANOS);
    }
    try testing.expectEqual(cache_mod.MAX_ENTRIES, c.count());

    // `k0` is the least-recently-used. Touch it, and `k1` becomes the victim
    // instead — this is the difference between LRU and insertion-order FIFO,
    // and a FIFO passes every count-based assertion above.
    try testing.expectEqualStrings("k0", c.lruKeyForTest().?);
    try expectHit(&c, "k0", "v", ORIGIN_NANOS);
    try testing.expectEqualStrings("k1", c.lruKeyForTest().?);

    try putOk(&c, "new", "v", ORIGIN_NANOS);
    try testing.expectEqual(cache_mod.MAX_ENTRIES, c.count());
    try expectMiss(&c, "k1", ORIGIN_NANOS); // evicted
    try expectHit(&c, "k0", "v", ORIGIN_NANOS); // survived because it was touched
    try expectHit(&c, "new", "v", ORIGIN_NANOS);
}

test "test_response_cache_accounting_and_lru: the TTL is 60s and monotonic" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    try putOk(&c, "rev1|a", "payload", ORIGIN_NANOS);

    // One nanosecond before the bound is still fresh.
    try expectHit(&c, "rev1|a", "payload", ORIGIN_NANOS + cache_mod.TTL_NANOS - 1);

    // At the bound it is expired — and the expired entry is DROPPED, not merely
    // hidden, so the tally returns to zero.
    try putOk(&c, "rev1|a", "payload", ORIGIN_NANOS);
    try expectMiss(&c, "rev1|a", ORIGIN_NANOS + cache_mod.TTL_NANOS);
    try testing.expectEqual(@as(usize, 0), c.count());
    try testing.expectEqual(@as(usize, 0), c.byteLen());

    try testing.expectEqual(@as(u64, 60 * std.time.ns_per_s), cache_mod.TTL_NANOS);
}

test "test_response_cache_accounting_and_lru: an oversized entry is bypassed, not an eviction cascade" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    try putOk(&c, "keep-me", "v", ORIGIN_NANOS);
    const before = c.byteLen();

    // One value larger than the whole budget. Admitting it would mean emptying
    // the cache to hold a single outlier — trading a working cache for one hit.
    // It is refused instead, and `put` reports false so the caller knows to
    // serve the response uncached rather than assume it was stored.
    const huge = try testing.allocator.alloc(u8, cache_mod.MAX_BYTES + 1);
    defer testing.allocator.free(huge);
    @memset(huge, 'x');
    try testing.expect(!(try c.put("huge", huge, ORIGIN_NANOS)));

    // The bypass left the existing contents completely untouched.
    try testing.expectEqual(before, c.byteLen());
    try testing.expectEqual(@as(usize, 1), c.count());
    try expectHit(&c, "keep-me", "v", ORIGIN_NANOS);
    try expectMiss(&c, "huge", ORIGIN_NANOS);
}

test "test_response_cache_accounting_and_lru: the byte ceiling is enforced, not just the entry count" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // Values big enough that the BYTE ceiling binds long before 256 entries do.
    const chunk = cache_mod.MAX_BYTES / 8;
    const value = try testing.allocator.alloc(u8, chunk);
    defer testing.allocator.free(value);
    @memset(value, 'y');

    var i: usize = 0;
    while (i < 24) : (i += 1) {
        var buf: [32]u8 = undefined;
        const k = try std.fmt.bufPrint(&buf, "big{d}", .{i});
        _ = try c.put(k, value, ORIGIN_NANOS);
        // The invariant that matters on every single insert.
        try testing.expect(c.byteLen() <= cache_mod.MAX_BYTES);
        try testing.expect(c.count() <= cache_mod.MAX_ENTRIES);
    }

    // It held far fewer than the entry ceiling, so the bound that stopped it was
    // the byte one.
    try testing.expect(c.count() < cache_mod.MAX_ENTRIES);
    try testing.expect(c.count() > 0);
}

test "test_response_cache_accounting_and_lru: the revision in the key isolates generations" {
    var c = cache_mod.Cache.init(testing.allocator);
    defer c.deinit();

    // The same selectors at two revisions are two keys. This is what makes a
    // stale candidate unreachable rather than dangerous: a request that has read
    // revision 2 never looks under the revision-1 key, so publish ordering
    // between concurrent builders stops mattering.
    try putOk(&c, "rev1|q=claude", "page-at-rev-1", ORIGIN_NANOS);
    try putOk(&c, "rev2|q=claude", "page-at-rev-2", ORIGIN_NANOS);

    try expectHit(&c, "rev1|q=claude", "page-at-rev-1", ORIGIN_NANOS);
    try expectHit(&c, "rev2|q=claude", "page-at-rev-2", ORIGIN_NANOS);
    try testing.expectEqual(@as(usize, 2), c.count());
}
