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
//!
//! — and deliberately do NOT pin which entry is evicted under pressure beyond
//! "the least recently used one", so the eviction policy stays free to change.
//!
//! The context below hashes to the key itself, so bucket placement is exact and
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
const BASE_MS: i64 = 1_000;

fn newTable() Table {
    return Table.init(.{});
}

/// Two keys that share a bucket, and one that does not.
const KEY_A: u64 = 1;
const KEY_A_COLLIDES: u64 = 1 + BUCKET_COUNT;
const KEY_A_COLLIDES_2: u64 = 1 + 2 * BUCKET_COUNT;
const KEY_OTHER_BUCKET: u64 = 2;

test "peek and get return a stored value" {
    var t = newTable();
    _ = t.put(KEY_A, 42, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(?u64, 42), t.peek(KEY_A, BASE_MS));
    try std.testing.expectEqual(@as(?u64, 42), t.get(KEY_A, BASE_MS));
}

test "absent key returns null from both readers" {
    var t = newTable();
    _ = t.put(KEY_A, 42, NEVER, BASE_MS);

    try std.testing.expect(t.peek(KEY_OTHER_BUCKET, BASE_MS) == null);
    try std.testing.expect(t.get(KEY_OTHER_BUCKET, BASE_MS) == null);
    // A key that hashes to the SAME bucket as a stored one must still miss —
    // this is the wrong-hit case, the one failure a cache cannot recover from.
    try std.testing.expect(t.peek(KEY_A_COLLIDES, BASE_MS) == null);
    try std.testing.expect(t.get(KEY_A_COLLIDES, BASE_MS) == null);
}

test "empty table returns null rather than reading uninitialized slots" {
    var t = newTable();
    for (0..BUCKET_COUNT * 4) |k| {
        try std.testing.expect(t.peek(@intCast(k), BASE_MS) == null);
    }
}

test "a value is returned up to its deadline and not on it" {
    var t = newTable();
    const ttl_ms: i64 = 10_000;
    _ = t.put(KEY_A, 42, BASE_MS + ttl_ms, BASE_MS);

    try std.testing.expectEqual(@as(?u64, 42), t.peek(KEY_A, BASE_MS + ttl_ms - 1));
    // Expiry is a deadline, not a grace period: at exactly expires_at_ms it is gone.
    try std.testing.expect(t.peek(KEY_A, BASE_MS + ttl_ms) == null);
    try std.testing.expect(t.peek(KEY_A, BASE_MS + ttl_ms + 60_000) == null);
}

test "get drops the expired entry it walked past" {
    var t = newTable();
    _ = t.put(KEY_A, 42, BASE_MS + 1, BASE_MS);

    try std.testing.expect(t.get(KEY_A, BASE_MS + 1) == null);
    // Freed by the read, so the slot is available again before any put.
    try std.testing.expectEqual(@as(usize, 0), t.count(BASE_MS));
}

test "peek leaves an expired entry in place" {
    var t = newTable();
    _ = t.put(KEY_A, 42, BASE_MS + 1, BASE_MS);

    try std.testing.expect(t.peek(KEY_A, BASE_MS + 1) == null);
    // peek is the read-lock-safe reader, so it must not mutate. The entry is
    // still occupying its slot; it is simply never returned.
    try std.testing.expectEqual(@as(usize, 0), t.count(BASE_MS + 1));
    try std.testing.expectEqual(@as(usize, 1), t.count(BASE_MS));
}

test "put overwrites the same key in place rather than adding a second entry" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A, 2, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A, BASE_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(BASE_MS));
}

test "put prefers the key's own live entry over an expired stranger earlier in the bucket" {
    // The intersection the two tests around this one miss: an EXPIRED foreign
    // entry sits at a lower index than the key's own LIVE entry. Taking the
    // first reusable slot would write the fresh value into the stranger's slot
    // and leave TWO entries for one key — and a later `remove` would drop only
    // one of them, leaving a stale duplicate answering after an invalidation.
    var t = newTable();
    _ = t.put(KEY_A_COLLIDES, 9, BASE_MS + 1, BASE_MS); // dies at BASE_MS+1, index 0
    _ = t.put(KEY_A, 1, NEVER, BASE_MS); // lives forever, index 1

    const later = BASE_MS + 10; // the stranger is now expired
    _ = t.put(KEY_A, 2, NEVER, later);

    try std.testing.expectEqual(@as(?u64, 2), t.get(KEY_A, later));
    try std.testing.expect(t.remove(KEY_A));
    // ONE remove fully removes: no stale duplicate may keep answering.
    try std.testing.expect(t.get(KEY_A, later) == null);
}

test "an expired entry is reused before a live one is evicted" {
    var t = newTable();
    // Fill the bucket: one that dies early, one that lives.
    _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);

    const now = BASE_MS + 5_000; // KEY_A is expired by now, KEY_A_COLLIDES is not.
    const evicted = t.put(KEY_A_COLLIDES_2, 3, NEVER, now);

    // The dead entry gave up its slot, so nothing live was displaced.
    try std.testing.expect(evicted == null);
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, now));
    try std.testing.expectEqual(@as(?u64, 3), t.peek(KEY_A_COLLIDES_2, now));
}

test "colliding keys coexist up to bucket_size" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);

    // The direct-mapped shape this replaces would have evicted one of these.
    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, BASE_MS));
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, BASE_MS));
}

test "overflowing a bucket evicts the least recently used entry" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);

    // Touch KEY_A so KEY_A_COLLIDES becomes the least recently used.
    try std.testing.expectEqual(@as(?u64, 1), t.get(KEY_A, BASE_MS));

    const evicted = t.put(KEY_A_COLLIDES_2, 3, NEVER, BASE_MS);
    try std.testing.expect(evicted != null);
    try std.testing.expectEqual(KEY_A_COLLIDES, evicted.?.key);

    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, BASE_MS));
    try std.testing.expect(t.peek(KEY_A_COLLIDES, BASE_MS) == null);
    try std.testing.expectEqual(@as(?u64, 3), t.peek(KEY_A_COLLIDES_2, BASE_MS));
}

test "peek does not refresh LRU position" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);

    // peek must not mutate, so KEY_A stays least-recently-used despite this.
    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, BASE_MS));

    const evicted = t.put(KEY_A_COLLIDES_2, 3, NEVER, BASE_MS);
    try std.testing.expectEqual(KEY_A, evicted.?.key);
}

test "pressure on one bucket leaves other buckets untouched" {
    var t = newTable();
    _ = t.put(KEY_OTHER_BUCKET, 99, NEVER, BASE_MS);

    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES_2, 3, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(?u64, 99), t.peek(KEY_OTHER_BUCKET, BASE_MS));
}

test "remove drops the entry and reports whether one was there" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);

    try std.testing.expect(t.remove(KEY_A));
    try std.testing.expect(t.peek(KEY_A, BASE_MS) == null);
    try std.testing.expect(!t.remove(KEY_A));
}

test "remove compacts a bucket without disturbing its other entry" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);

    try std.testing.expect(t.remove(KEY_A));
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES, BASE_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(BASE_MS));
}

test "remove of an expired entry still reports it was present" {
    var t = newTable();
    _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);

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
    _ = t.put(KEY_A, 7, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 7, NEVER, BASE_MS);
    _ = t.put(KEY_OTHER_BUCKET, 8, NEVER, BASE_MS);

    const removed = t.removeMatching(ValuePredicate{ .wanted = 7 });

    try std.testing.expectEqual(@as(usize, 2), removed);
    try std.testing.expect(t.peek(KEY_A, BASE_MS) == null);
    try std.testing.expect(t.peek(KEY_A_COLLIDES, BASE_MS) == null);
    try std.testing.expectEqual(@as(?u64, 8), t.peek(KEY_OTHER_BUCKET, BASE_MS));
}

test "removeMatching that matches nothing removes nothing" {
    var t = newTable();
    _ = t.put(KEY_A, 7, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(usize, 0), t.removeMatching(ValuePredicate{ .wanted = 9 }));
    try std.testing.expectEqual(@as(?u64, 7), t.peek(KEY_A, BASE_MS));
}

test "clear empties every bucket" {
    var t = newTable();
    for (0..BUCKET_COUNT * BUCKET_SIZE) |k| _ = t.put(@intCast(k), 1, NEVER, BASE_MS);

    t.clear();

    try std.testing.expectEqual(@as(usize, 0), t.count(BASE_MS));
    for (0..BUCKET_COUNT * BUCKET_SIZE) |k| {
        try std.testing.expect(t.peek(@intCast(k), BASE_MS) == null);
    }
}

test "count reports live entries only" {
    var t = newTable();
    _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);
    _ = t.put(KEY_OTHER_BUCKET, 2, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(usize, 2), t.count(BASE_MS));
    try std.testing.expectEqual(@as(usize, 1), t.count(BASE_MS + 1));
}

test "NEVER_EXPIRES survives any plausible clock" {
    var t = newTable();
    _ = t.put(KEY_A, 1, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A, std.math.maxInt(i64) - 1));
}

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

test "every path that drops an entry hands it to the eviction hook" {
    // A value type that owns memory leaks through any drop path the table
    // forgets to route through the hook. These are the four that are not
    // "bucket overflowed", and each one was silently dropping its occupant.
    const cases = [_]struct {
        name: []const u8,
        run: *const fn (*SpyTable) void,
    }{
        .{ .name = "put refreshing an existing key releases the old value", .run = struct {
            fn f(t: *SpyTable) void {
                _ = t.put(KEY_A, 1, NEVER, BASE_MS);
                _ = t.put(KEY_A, 2, NEVER, BASE_MS);
            }
        }.f },
        .{ .name = "put reusing an expired slot releases its occupant", .run = struct {
            fn f(t: *SpyTable) void {
                _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);
                _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS + 5_000);
            }
        }.f },
        .{ .name = "get dropping an expired entry releases it", .run = struct {
            fn f(t: *SpyTable) void {
                _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);
                _ = t.get(KEY_A, BASE_MS + 1);
            }
        }.f },
        .{ .name = "remove releases the entry", .run = struct {
            fn f(t: *SpyTable) void {
                _ = t.put(KEY_A, 1, NEVER, BASE_MS);
                _ = t.remove(KEY_A);
            }
        }.f },
        .{ .name = "removeMatching releases each entry it drops", .run = struct {
            fn f(t: *SpyTable) void {
                _ = t.put(KEY_A, 7, NEVER, BASE_MS);
                _ = t.removeMatching(ValuePredicate{ .wanted = 7 });
            }
        }.f },
    };

    for (cases) |case| {
        EvictionSpy.reset();
        var t: SpyTable = SpyTable.init(.{});
        case.run(&t);
        std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count) catch |err| {
            std.debug.print("leaked drop path: {s}\n", .{case.name});
            return err;
        };
    }
}

test "the eviction hook fires on overflow and on clear" {
    EvictionSpy.reset();
    var t: SpyTable = SpyTable.init(.{});

    _ = t.put(KEY_A, 1, NEVER, BASE_MS);
    _ = t.put(KEY_A_COLLIDES, 2, NEVER, BASE_MS);
    try std.testing.expectEqual(@as(usize, 0), EvictionSpy.seen_count);

    _ = t.put(KEY_A_COLLIDES_2, 3, NEVER, BASE_MS);
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);

    t.clear();
    try std.testing.expectEqual(@as(usize, 3), EvictionSpy.seen_count);
}

test "sweepExpired releases the dead and keeps the live" {
    EvictionSpy.reset();
    var t: SpyTable = SpyTable.init(.{});
    _ = t.put(KEY_A, 1, BASE_MS + 1, BASE_MS);
    _ = t.put(KEY_OTHER_BUCKET, 2, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(usize, 1), t.sweepExpired(BASE_MS + 1));
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_OTHER_BUCKET, BASE_MS + 1));

    // Idempotent: the dead are already gone, so a second sweep finds nothing.
    try std.testing.expectEqual(@as(usize, 0), t.sweepExpired(BASE_MS + 1));
}

// ---------------------------------------------------------------------------
// The release rule, proved by an allocator instead of a counter.
// ---------------------------------------------------------------------------

/// Frees the value of every entry that leaves the table. This is the whole
/// reason the release rule has to be exhaustive: `std.testing.allocator`
/// reports a leak for any path that forgets to call it, and a double-free for
/// any path that calls it twice — neither of which a hook counter can see.
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

test "sweepExpired reclaims the memory an expired value was still holding" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });
    defer t.clear();

    // Nothing else will ever reach these two: expiry is lazy, so without the
    // sweep their bytes stay held until the slot is reused or the table dies.
    _ = t.put(KEY_A, try ownedValue(alloc, 'a'), BASE_MS + 1, BASE_MS);
    _ = t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'b'), BASE_MS + 1, BASE_MS);

    try std.testing.expectEqual(@as(usize, 2), t.sweepExpired(BASE_MS + 1));
    try std.testing.expectEqual(@as(usize, 0), t.count(BASE_MS + 1));
}

test "every departure path frees an owned value" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });
    defer t.clear();

    // Same-key refresh: releases the value it replaced.
    _ = t.put(KEY_A, try ownedValue(alloc, 'a'), NEVER, BASE_MS);
    _ = t.put(KEY_A, try ownedValue(alloc, 'b'), NEVER, BASE_MS);

    // Expired-slot reuse: the bucket's second slot dies, then is taken over.
    _ = t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'c'), BASE_MS + 1, BASE_MS);
    _ = t.put(KEY_A_COLLIDES_2, try ownedValue(alloc, 'd'), NEVER, BASE_MS + 2);

    // Bucket overflow: both slots are live, so the least recent one goes. The
    // returned entry has already been released — its value must not be read.
    _ = t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'e'), NEVER, BASE_MS + 2);

    // Explicit removal.
    try std.testing.expect(t.remove(KEY_A_COLLIDES_2));

    // Reaped by a get that found it expired.
    _ = t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'f'), BASE_MS + 3, BASE_MS);
    try std.testing.expect(t.get(KEY_OTHER_BUCKET, BASE_MS + 3) == null);

    // Removal by predicate.
    _ = t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'g'), NEVER, BASE_MS);
    try std.testing.expectEqual(@as(usize, 2), t.removeMatching(AllPredicate{}));

    // Anything still resident is freed by the deferred clear; the testing
    // allocator fails this test if any of the above missed its release.
}

test "clear frees the owned values still resident" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });

    _ = t.put(KEY_A, try ownedValue(alloc, 'a'), NEVER, BASE_MS);
    _ = t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'b'), NEVER, BASE_MS);

    t.clear();
    try std.testing.expectEqual(@as(usize, 0), t.count(BASE_MS));
}

test "a full table keeps answering correctly for everything still resident" {
    var t = newTable();
    const total = BUCKET_COUNT * BUCKET_SIZE;
    for (0..total) |k| _ = t.put(@intCast(k), @as(u64, @intCast(k)) * 10, NEVER, BASE_MS);

    try std.testing.expectEqual(@as(usize, total), t.count(BASE_MS));
    for (0..total) |k| {
        try std.testing.expectEqual(@as(?u64, @as(u64, @intCast(k)) * 10), t.peek(@intCast(k), BASE_MS));
    }
}

test "churning far more keys than capacity never yields a wrong value" {
    var t = newTable();
    const CHURN: u64 = 500;

    var k: u64 = 0;
    while (k < CHURN) : (k += 1) {
        _ = t.put(k, k * 3, NEVER, BASE_MS);
        // Whatever survived must still map to its own value. A cache may forget;
        // it may never confuse two keys.
        var probe: u64 = 0;
        while (probe <= k) : (probe += 1) {
            if (t.peek(probe, BASE_MS)) |v| try std.testing.expectEqual(probe * 3, v);
        }
    }
    try std.testing.expect(t.count(BASE_MS) <= BUCKET_COUNT * BUCKET_SIZE);
}
