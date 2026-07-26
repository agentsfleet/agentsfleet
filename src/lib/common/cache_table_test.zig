//! Tests for `cache_table.zig`.
//!
//! The table is a cache, so most of its behaviour is a *permission* rather than
//! a promise: it MAY drop anything at any time without breaking a caller. These
//! tests pin the parts that are genuinely load-bearing —
//!
//!   1. it never returns a value for a key that was not stored (a wrong hit is
//!      the only failure a cache cannot recover from),
//!   2. it never returns an expired value,
//!   3. an expired entry never costs a live entry its slot,
//!   4. removal actually removes, including by predicate,
//!   5. every entry that leaves the table is released exactly once,
//!
//! — and deliberately do NOT pin which entry is evicted under pressure beyond
//! "the least recently used one", so the eviction policy stays free to change.
//!
//! (5) is the rule that lets `V` own memory, so it is tested twice: once
//! through a counting spy that names the departing key on each path, and once
//! against `std.testing.allocator` with `V = []u8`, where a missed release is a
//! reported leak rather than an assertion someone has to think to write.
//!
//! The contexts below hash to the key itself, so bucket placement is exact and
//! collision cases are constructible rather than hoped for.

const std = @import("std");
const cache_table = @import("cache_table.zig");

const BUCKET_COUNT: usize = 4;
const BUCKET_SIZE: u8 = 2;

/// Identity hash: key `k` lands in bucket `k % BUCKET_COUNT`, so any two keys
/// differing by a multiple of BUCKET_COUNT collide on purpose.
const IdentityContext = struct {
    pub fn hash(_: *const IdentityContext, key: u64) u64 {
        return key;
    }
    pub fn eql(_: *const IdentityContext, a: u64, b: u64) bool {
        return a == b;
    }
};

const Table = cache_table.CacheTable(u64, u64, IdentityContext, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = BUCKET_SIZE,
});

const NEVER = cache_table.NEVER_EXPIRES;

/// The instant every test reads "as of" unless it is stepping time deliberately.
/// Non-zero on purpose, so a bug treating a zero timestamp as "unset" cannot pass.
const ORIGIN_MS: i64 = 1_000;

fn newTable() Table {
    return Table.init(.{});
}

/// Two keys that share a bucket, and one that does not.
const KEY_A: u64 = 1;
const KEY_A_COLLIDES: u64 = 1 + BUCKET_COUNT;
const KEY_A_COLLIDES_2: u64 = 1 + 2 * BUCKET_COUNT;
const KEY_OTHER_BUCKET: u64 = 2;

/// Records every key the table releases, so the departure paths are assertable
/// by name rather than inferred from which later lookups happen to miss.
const EvictionSpy = struct {
    var seen_keys: [8]u64 = @splat(0);
    var seen_count: usize = 0;

    pub fn hash(_: *const EvictionSpy, key: u64) u64 {
        return key;
    }
    pub fn eql(_: *const EvictionSpy, a: u64, b: u64) bool {
        return a == b;
    }
    pub fn evicted(_: *const EvictionSpy, key: u64, _: u64) void {
        if (seen_count < seen_keys.len) seen_keys[seen_count] = key;
        seen_count += 1;
    }

    fn reset() void {
        seen_count = 0;
        seen_keys = @splat(0);
    }
};

const SpyTable = cache_table.CacheTable(u64, u64, EvictionSpy, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = BUCKET_SIZE,
});

fn newSpyTable() SpyTable {
    EvictionSpy.reset();
    return SpyTable.init(.{});
}

test "peek and get return a stored value" {
    var t = newTable();
    t.put(KEY_A, 42, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(?u64, 42), t.peek(KEY_A, ORIGIN_MS));
    try std.testing.expectEqual(@as(?u64, 42), t.get(KEY_A, ORIGIN_MS));
}

test "absent key returns null from both readers" {
    var t = newTable();
    t.put(KEY_A, 42, NEVER, ORIGIN_MS);

    try std.testing.expect(t.peek(KEY_OTHER_BUCKET, ORIGIN_MS) == null);
    try std.testing.expect(t.get(KEY_OTHER_BUCKET, ORIGIN_MS) == null);
    // A key that hashes to the SAME bucket as a stored one must still miss —
    // this is the wrong-hit case, the one failure a cache cannot recover from.
    try std.testing.expect(t.peek(KEY_A_COLLIDES, ORIGIN_MS) == null);
    try std.testing.expect(t.get(KEY_A_COLLIDES, ORIGIN_MS) == null);
}

test "empty table returns null rather than reading uninitialized slots" {
    var t = newTable();
    for (0..BUCKET_COUNT * 4) |k| {
        try std.testing.expect(t.peek(@intCast(k), ORIGIN_MS) == null);
    }
}

test "a value is returned up to its deadline and not on it" {
    var t = newTable();
    const ttl_ms: i64 = 10_000;
    t.put(KEY_A, 42, ORIGIN_MS + ttl_ms, ORIGIN_MS);

    try std.testing.expectEqual(@as(?u64, 42), t.peek(KEY_A, ORIGIN_MS + ttl_ms - 1));
    // Expiry is a deadline, not a grace period: at exactly expires_at_ms it is gone.
    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS + ttl_ms) == null);
    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS + ttl_ms + 60_000) == null);
}

test "get drops the expired entry it walked past" {
    var t = newTable();
    t.put(KEY_A, 42, ORIGIN_MS + 1, ORIGIN_MS);

    try std.testing.expect(t.get(KEY_A, ORIGIN_MS + 1) == null);
    // Freed by the read, so the slot is available again before any put.
    try std.testing.expectEqual(@as(usize, 0), t.count(ORIGIN_MS));
}

test "peek leaves an expired entry in place" {
    var t = newTable();
    t.put(KEY_A, 42, ORIGIN_MS + 1, ORIGIN_MS);

    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS + 1) == null);
    // peek is the read-lock-safe reader, so it must not mutate. The entry is
    // still occupying its slot; it is simply never returned.
    try std.testing.expectEqual(@as(usize, 0), t.count(ORIGIN_MS + 1));
    try std.testing.expectEqual(@as(usize, 1), t.count(ORIGIN_MS));
}

test "put overwrites the same key in place rather than adding a second entry" {
    var t = newTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A, 2, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A, ORIGIN_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(ORIGIN_MS));
}

test "an expired entry is reused before a live one is evicted" {
    var t = newSpyTable();
    // Fill the bucket: one that dies early, one that lives.
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);

    const now = ORIGIN_MS + 5_000; // KEY_A is expired by now, KEY_A_COLLIDES is not.
    t.put(KEY_A_COLLIDES_2, 3, NEVER, now);

    // The dead entry gave up its slot, so nothing live was displaced...
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, now));
    try std.testing.expectEqual(@as(?u64, 3), t.peek(KEY_A_COLLIDES_2, now));
    // ...and it was still released on its way out. Expiry is lazy, so nothing
    // else ever reaches that entry; if this path stayed silent, an owning
    // consumer would lose the value rather than free it.
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
}

test "colliding keys coexist up to bucket_size" {
    var t = newTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);

    // The direct-mapped shape this replaces would have evicted one of these.
    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, ORIGIN_MS));
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, ORIGIN_MS));
}

test "overflowing a bucket releases the least recently used entry" {
    var t = newSpyTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);

    // Touch KEY_A so KEY_A_COLLIDES becomes the least recently used.
    try std.testing.expectEqual(@as(?u64, 1), t.get(KEY_A, ORIGIN_MS));

    t.put(KEY_A_COLLIDES_2, 3, NEVER, ORIGIN_MS);
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A_COLLIDES, EvictionSpy.seen_keys[0]);

    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, ORIGIN_MS));
    try std.testing.expect(t.peek(KEY_A_COLLIDES, ORIGIN_MS) == null);
    try std.testing.expectEqual(@as(?u64, 3), t.peek(KEY_A_COLLIDES_2, ORIGIN_MS));
}

test "peek does not refresh LRU position" {
    var t = newSpyTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);

    // peek must not mutate, so KEY_A stays least-recently-used despite this.
    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, ORIGIN_MS));

    t.put(KEY_A_COLLIDES_2, 3, NEVER, ORIGIN_MS);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
}

test "pressure on one bucket leaves other buckets untouched" {
    var t = newTable();
    t.put(KEY_OTHER_BUCKET, 99, NEVER, ORIGIN_MS);

    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES_2, 3, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(?u64, 99), t.peek(KEY_OTHER_BUCKET, ORIGIN_MS));
}

test "remove drops the entry and reports whether one was there" {
    var t = newTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);

    try std.testing.expect(t.remove(KEY_A));
    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS) == null);
    try std.testing.expect(!t.remove(KEY_A));
}

test "remove compacts a bucket without disturbing its other entry" {
    var t = newTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);

    try std.testing.expect(t.remove(KEY_A));
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, ORIGIN_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(ORIGIN_MS));
}

test "remove of an expired entry still reports it was present" {
    var t = newTable();
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);

    // Removal is about occupancy, not liveness — an expired entry is still a
    // row that has to go, and the caller learns it existed.
    try std.testing.expect(t.remove(KEY_A));
}

const ValuePredicate = struct {
    wanted: u64,
    pub fn match(self: ValuePredicate, _: u64, value: u64) bool {
        return value == self.wanted;
    }
};

test "removeMatching drops every entry the predicate accepts, across buckets" {
    var t = newTable();
    t.put(KEY_A, 7, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 7, NEVER, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, 8, NEVER, ORIGIN_MS);

    const removed = t.removeMatching(ValuePredicate{ .wanted = 7 });

    try std.testing.expectEqual(@as(usize, 2), removed);
    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS) == null);
    try std.testing.expect(t.peek(KEY_A_COLLIDES, ORIGIN_MS) == null);
    try std.testing.expectEqual(@as(?u64, 8), t.peek(KEY_OTHER_BUCKET, ORIGIN_MS));
}

test "removeMatching that matches nothing removes nothing" {
    var t = newTable();
    t.put(KEY_A, 7, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, 0), t.removeMatching(ValuePredicate{ .wanted = 9 }));
    try std.testing.expectEqual(@as(?u64, 7), t.peek(KEY_A, ORIGIN_MS));
}

test "clear empties every bucket" {
    var t = newTable();
    for (0..BUCKET_COUNT * BUCKET_SIZE) |k| t.put(@intCast(k), 1, NEVER, ORIGIN_MS);

    t.clear();

    try std.testing.expectEqual(@as(usize, 0), t.count(ORIGIN_MS));
    for (0..BUCKET_COUNT * BUCKET_SIZE) |k| {
        try std.testing.expect(t.peek(@intCast(k), ORIGIN_MS) == null);
    }
}

test "count reports live entries only" {
    var t = newTable();
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, 2, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, 2), t.count(ORIGIN_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(ORIGIN_MS + 1));
}

test "NEVER_EXPIRES survives any plausible clock" {
    var t = newTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, std.math.maxInt(i64) - 1));
}

// ---------------------------------------------------------------------------
// The release rule, path by path.
//
// Only two of these paths reported anything before the rule was made
// exhaustive. The rest dropped their entry in silence, which is invisible when
// `V` is a plain integer and a leak the moment it owns memory.
// ---------------------------------------------------------------------------

test "overwriting a key releases the value it replaced" {
    var t = newSpyTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A, 2, NEVER, ORIGIN_MS);

    // The most common write a time-to-live cache makes: the same key, refreshed.
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A, ORIGIN_MS));
}

test "remove releases the entry it drops" {
    var t = newSpyTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);

    try std.testing.expect(t.remove(KEY_A));
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);

    // A remove that found nothing releases nothing.
    try std.testing.expect(!t.remove(KEY_A));
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
}

test "removeMatching releases every entry it drops" {
    var t = newSpyTable();
    t.put(KEY_A, 7, NEVER, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, 7, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, 2), t.removeMatching(ValuePredicate{ .wanted = 7 }));
    try std.testing.expectEqual(@as(usize, 2), EvictionSpy.seen_count);
}

test "a get that reaps an expired entry releases it" {
    var t = newSpyTable();
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);

    try std.testing.expect(t.get(KEY_A, ORIGIN_MS + 1) == null);
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
}

test "peek releases nothing, even when it declines an expired entry" {
    var t = newSpyTable();
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);

    try std.testing.expect(t.peek(KEY_A, ORIGIN_MS + 1) == null);
    // peek is the shared-lock reader. Releasing here would mutate under a lock
    // that permits concurrent readers, and could free a value another reader
    // is still holding.
    try std.testing.expectEqual(@as(usize, 0), EvictionSpy.seen_count);
}

test "sweepExpired releases the dead and keeps the live" {
    var t = newSpyTable();
    t.put(KEY_A, 1, ORIGIN_MS + 1, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, 2, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, 1), t.sweepExpired(ORIGIN_MS + 1));
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_OTHER_BUCKET, ORIGIN_MS + 1));

    // Idempotent: the dead are already gone, so a second sweep finds nothing.
    try std.testing.expectEqual(@as(usize, 0), t.sweepExpired(ORIGIN_MS + 1));
}

test "sweepExpired reclaims the memory an expired value was still holding" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });
    defer t.clear();

    // Nothing else will ever reach these two: expiry is lazy, so without the
    // sweep their bytes stay held until the slot is reused or the table dies.
    t.put(KEY_A, try ownedValue(alloc, 'a'), ORIGIN_MS + 1, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'b'), ORIGIN_MS + 1, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, 2), t.sweepExpired(ORIGIN_MS + 1));
    try std.testing.expectEqual(@as(usize, 0), t.count(ORIGIN_MS + 1));
}

test "clear releases every resident entry" {
    var t = newSpyTable();
    t.put(KEY_A, 1, NEVER, ORIGIN_MS);
    t.put(KEY_A_COLLIDES, 2, NEVER, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, 3, NEVER, ORIGIN_MS);

    t.clear();
    try std.testing.expectEqual(@as(usize, 3), EvictionSpy.seen_count);

    // Cleared entries are gone, so a second clear releases nothing — the rule
    // is "exactly once", not "at least once".
    t.clear();
    try std.testing.expectEqual(@as(usize, 3), EvictionSpy.seen_count);
}

// ---------------------------------------------------------------------------
// The same rule, proved by an allocator instead of a counter.
// ---------------------------------------------------------------------------

/// Frees the value of every entry that leaves the table. This is the whole
/// reason the release rule has to be exhaustive: `std.testing.allocator`
/// reports a leak for any path that forgets to call it, and a double-free for
/// any path that calls it twice.
const OwnedContext = struct {
    alloc: std.mem.Allocator,

    pub fn hash(_: *const OwnedContext, key: u64) u64 {
        return key;
    }
    pub fn eql(_: *const OwnedContext, a: u64, b: u64) bool {
        return a == b;
    }
    pub fn evicted(self: *const OwnedContext, _: u64, value: []u8) void {
        self.alloc.free(value);
    }
};

const OwnedTable = cache_table.CacheTable(u64, []u8, OwnedContext, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = BUCKET_SIZE,
});

const OWNED_VALUE_BYTES: usize = 32;

fn ownedValue(alloc: std.mem.Allocator, fill: u8) ![]u8 {
    const buf = try alloc.alloc(u8, OWNED_VALUE_BYTES);
    @memset(buf, fill);
    return buf;
}

const AllPredicate = struct {
    pub fn match(_: AllPredicate, _: u64, _: []u8) bool {
        return true;
    }
};

test "every departure path frees an owned value" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });
    defer t.clear();

    // Same-key refresh: releases the value it replaced.
    t.put(KEY_A, try ownedValue(alloc, 'a'), NEVER, ORIGIN_MS);
    t.put(KEY_A, try ownedValue(alloc, 'b'), NEVER, ORIGIN_MS);

    // Expired-slot reuse: the bucket's second slot dies, then is taken over.
    t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'c'), ORIGIN_MS + 1, ORIGIN_MS);
    t.put(KEY_A_COLLIDES_2, try ownedValue(alloc, 'd'), NEVER, ORIGIN_MS + 2);

    // Bucket overflow: both slots are live, so the least recent one goes.
    t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'e'), NEVER, ORIGIN_MS + 2);

    // Explicit removal.
    try std.testing.expect(t.remove(KEY_A_COLLIDES_2));

    // Reaped by a get that found it expired.
    t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'f'), ORIGIN_MS + 3, ORIGIN_MS);
    try std.testing.expect(t.get(KEY_OTHER_BUCKET, ORIGIN_MS + 3) == null);

    // Removal by predicate.
    t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'g'), NEVER, ORIGIN_MS);
    try std.testing.expectEqual(@as(usize, 2), t.removeMatching(AllPredicate{}));

    // Anything still resident is freed by the deferred clear; the testing
    // allocator fails this test if any of the above missed its release.
}

test "clear frees the owned values still resident" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });

    t.put(KEY_A, try ownedValue(alloc, 'a'), NEVER, ORIGIN_MS);
    t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'b'), NEVER, ORIGIN_MS);

    t.clear();
    try std.testing.expectEqual(@as(usize, 0), t.count(ORIGIN_MS));
}

test "a full table keeps answering correctly for everything still resident" {
    var t = newTable();
    const total = BUCKET_COUNT * BUCKET_SIZE;
    for (0..total) |k| t.put(@intCast(k), @as(u64, @intCast(k)) * 10, NEVER, ORIGIN_MS);

    try std.testing.expectEqual(@as(usize, total), t.count(ORIGIN_MS));
    for (0..total) |k| {
        try std.testing.expectEqual(@as(?u64, @as(u64, @intCast(k)) * 10), t.peek(@intCast(k), ORIGIN_MS));
    }
}

test "churning far more keys than capacity never yields a wrong value" {
    var t = newTable();
    const CHURN: u64 = 500;

    var k: u64 = 0;
    while (k < CHURN) : (k += 1) {
        t.put(k, k * 3, NEVER, ORIGIN_MS);
        // Whatever survived must still map to its own value. A cache may forget;
        // it may never confuse two keys.
        var probe: u64 = 0;
        while (probe <= k) : (probe += 1) {
            if (t.peek(probe, ORIGIN_MS)) |v| try std.testing.expectEqual(probe * 3, v);
        }
    }
    try std.testing.expect(t.count(ORIGIN_MS) <= BUCKET_COUNT * BUCKET_SIZE);
}
