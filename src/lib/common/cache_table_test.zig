//! Tests for `cache_table.zig`.
//!
//! The table is a cache, so most of its behaviour is a *permission* rather than
//! a promise: it MAY drop anything at any time without breaking a caller. These
//! tests pin the parts that are genuinely load-bearing —
//!
//!   1. it never returns a value for a key that was not stored (a wrong hit is
//!      the only failure a cache cannot recover from),
//!   2. when a key is resident twice, the NEWEST value answers,
//!   3. every entry that leaves is handed to the eviction hook exactly once,
//!
//! — and deliberately do NOT pin which entry is evicted beyond "this bucket's
//! oldest", so the policy stays free to change.
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

fn newTable() Table {
    return Table.init(.{});
}

/// Two keys that share a bucket, and one that does not.
const KEY_A: u64 = 1;
const KEY_A_COLLIDES: u64 = 1 + BUCKET_COUNT;
const KEY_A_COLLIDES_2: u64 = 1 + 2 * BUCKET_COUNT;
const KEY_OTHER_BUCKET: u64 = 2;

test "peek returns a stored value" {
    var t = newTable();
    t.put(KEY_A, 42);
    try std.testing.expectEqual(@as(?u64, 42), t.peek(KEY_A));
}

test "absent key returns null" {
    var t = newTable();
    t.put(KEY_A, 42);

    try std.testing.expect(t.peek(KEY_OTHER_BUCKET) == null);
    // A key that hashes to the SAME bucket as a stored one must still miss —
    // this is the wrong-hit case, the one failure a cache cannot recover from.
    try std.testing.expect(t.peek(KEY_A_COLLIDES) == null);
}

test "empty table returns null rather than reading uninitialized slots" {
    var t = newTable();
    for (0..BUCKET_COUNT * 4) |k| {
        try std.testing.expect(t.peek(@intCast(k)) == null);
    }
}

test "colliding keys coexist up to bucket_size" {
    var t = newTable();
    t.put(KEY_A, 1);
    t.put(KEY_A_COLLIDES, 2);

    try std.testing.expectEqual(@as(?u64, 1), t.peek(KEY_A));
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES));
    try std.testing.expectEqual(@as(usize, 2), t.count());
}

test "a re-put of the same key is answered by the newer value" {
    var t = newTable();
    t.put(KEY_A, 1);
    t.put(KEY_A, 2);

    // Both are resident — `put` appends rather than searching — so this pins
    // that `peek` scans from the newest end and the stale one stays unreachable.
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A));
    try std.testing.expectEqual(@as(usize, 2), t.count());
}

test "overflowing a bucket evicts its oldest entry" {
    var t = newTable();
    t.put(KEY_A, 1);
    t.put(KEY_A_COLLIDES, 2);
    t.put(KEY_A_COLLIDES_2, 3);

    try std.testing.expect(t.peek(KEY_A) == null); // oldest, gone
    try std.testing.expectEqual(@as(?u64, 2), t.peek(KEY_A_COLLIDES));
    try std.testing.expectEqual(@as(?u64, 3), t.peek(KEY_A_COLLIDES_2));
    try std.testing.expectEqual(@as(usize, BUCKET_SIZE), t.count());
}

test "pressure on one bucket leaves other buckets untouched" {
    var t = newTable();
    t.put(KEY_OTHER_BUCKET, 99);
    t.put(KEY_A, 1);
    t.put(KEY_A_COLLIDES, 2);
    t.put(KEY_A_COLLIDES_2, 3);

    try std.testing.expectEqual(@as(?u64, 99), t.peek(KEY_OTHER_BUCKET));
}

test "clear empties every bucket" {
    var t = newTable();
    t.put(KEY_A, 1);
    t.put(KEY_OTHER_BUCKET, 2);

    t.clear();
    try std.testing.expectEqual(@as(usize, 0), t.count());
    try std.testing.expect(t.peek(KEY_A) == null);
    try std.testing.expect(t.peek(KEY_OTHER_BUCKET) == null);
}

// ---------------------------------------------------------------------------
// The release rule, by a counting spy.
// ---------------------------------------------------------------------------

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

test "the eviction hook fires on overflow, naming the entry that left" {
    EvictionSpy.reset();
    var t: SpyTable = SpyTable.init(.{});

    t.put(KEY_A, 1);
    t.put(KEY_A_COLLIDES, 2);
    try std.testing.expectEqual(@as(usize, 0), EvictionSpy.seen_count);

    t.put(KEY_A_COLLIDES_2, 3);
    try std.testing.expectEqual(@as(usize, 1), EvictionSpy.seen_count);
    try std.testing.expectEqual(KEY_A, EvictionSpy.seen_keys[0]);
}

test "the eviction hook fires for every resident entry on clear, exactly once" {
    EvictionSpy.reset();
    var t: SpyTable = SpyTable.init(.{});
    t.put(KEY_A, 1);
    t.put(KEY_A_COLLIDES, 2);
    t.put(KEY_OTHER_BUCKET, 3);

    t.clear();
    try std.testing.expectEqual(@as(usize, 3), EvictionSpy.seen_count);

    // Cleared entries are gone, so a second clear releases nothing — the rule is
    // "exactly once", not "at least once".
    t.clear();
    try std.testing.expectEqual(@as(usize, 3), EvictionSpy.seen_count);
}

// ---------------------------------------------------------------------------
// The same rule, proved by an allocator instead of a counter.
// ---------------------------------------------------------------------------

/// Frees the value of every entry that leaves. `std.testing.allocator` reports a
/// leak for any exit that forgets to call this, and a double-free for any exit
/// that calls it twice — neither of which a hook counter can see.
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

test "eviction frees the owned value that lost its slot" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });
    defer t.clear();

    t.put(KEY_A, try ownedValue(alloc, 'a'));
    t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'b'));
    // Overflows the bucket: KEY_A's body must be freed here, not leaked.
    t.put(KEY_A_COLLIDES_2, try ownedValue(alloc, 'c'));

    // A same-key re-put does NOT free the older body — it appends, and the older
    // entry stays resident until evicted. Pinned so the deferred clear below is
    // understood to be what frees it.
    t.put(KEY_A_COLLIDES, try ownedValue(alloc, 'd'));

    // Anything still resident is freed by the deferred clear; the testing
    // allocator fails this test if any exit above missed its release.
}

test "clear frees the owned values still resident" {
    const alloc = std.testing.allocator;
    var t = OwnedTable.init(.{ .alloc = alloc });

    t.put(KEY_A, try ownedValue(alloc, 'a'));
    t.put(KEY_OTHER_BUCKET, try ownedValue(alloc, 'b'));

    t.clear();
    try std.testing.expectEqual(@as(usize, 0), t.count());
}

// ---------------------------------------------------------------------------
// Whole-table properties.
// ---------------------------------------------------------------------------

test "a full table keeps answering correctly for everything still resident" {
    var t = newTable();
    const total = BUCKET_COUNT * BUCKET_SIZE;
    for (0..total) |k| t.put(@intCast(k), @as(u64, @intCast(k)) * 10);

    try std.testing.expectEqual(@as(usize, total), t.count());
    for (0..total) |k| {
        try std.testing.expectEqual(@as(?u64, @as(u64, @intCast(k)) * 10), t.peek(@intCast(k)));
    }
}

test "churning far more keys than capacity never yields a wrong value" {
    var t = newTable();
    const CHURN: u64 = 500;

    var k: u64 = 0;
    while (k < CHURN) : (k += 1) {
        t.put(k, k * 3);
        // Whatever survived must still map to its own value. A cache may forget;
        // it may never confuse two keys.
        var probe: u64 = 0;
        while (probe <= k) : (probe += 1) {
            if (t.peek(probe)) |v| try std.testing.expectEqual(probe * 3, v);
        }
    }
    try std.testing.expect(t.count() <= BUCKET_COUNT * BUCKET_SIZE);
}
