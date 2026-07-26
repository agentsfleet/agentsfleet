//! Fixed-capacity associative cache: N-way set-associative buckets, least-
//! recently-used (LRU) eviction within a bucket, per-entry expiry, no allocator.
//!
//! For values that can always be recomputed. A miss is never an error — it costs
//! whatever the underlying lookup costs — so the table trades exactness of
//! retention for a hard memory bound and zero allocation.
//!
//! **Why set-associative and not direct-mapped.** A direct-mapped table has no
//! probing: two live keys landing on one slot evict each other on every
//! interleaved access and both fall back to the real lookup permanently, which
//! makes the cache worst at exactly the scale it exists for. Widening the table
//! only makes that rarer. `bucket_size` entries per bucket makes it *survivable*
//! — colliding keys coexist, and only the least recently used one leaves.
//!
//! **Not synchronized.** No lock is held internally, so a consumer picks a lock
//! matched to its own correctness needs rather than paying for the strictest
//! one. `peek` never mutates and is safe under a shared/read lock; `get`,
//! `put`, `remove`, `removeMatching`, and `clear` mutate and need exclusive
//! access.
//!
//! ## Attribution
//!
//! The bucket-and-lengths layout, in-bucket LRU by rotation, and the optional
//! eviction hook are adapted from Ghostty's `src/datastruct/cache_table.zig` —
//! MIT, Copyright (c) 2024 Mitchell Hashimoto, Ghostty contributors.
//!
//! Deriving the bucket index from a digest's leading bytes rather than re-hashing
//! an already-uniform key is from Bun's `src/runtime/api/bun/SSLContextCache.zig`
//! — MIT, Copyright (c) Oven. Applied by consumers in their `Context.hash`.
//!
//! Per-entry expiry, expired-slot reuse, `peek`, and `removeMatching` are not in
//! either upstream.

const std = @import("std");

/// Expiry for entries that live until something explicitly removes them.
/// `put(k, v, NEVER_EXPIRES)` is the no-time-bound case.
pub const NEVER_EXPIRES: i64 = std.math.maxInt(i64);

/// Optional `Context` declaration called whenever an entry leaves the table.
const EVICTION_HOOK = "evicted";

pub const Options = struct {
    /// Number of buckets. Power of two — the index is a mask, not a modulo.
    /// Size it near the count of keys expected live at once.
    bucket_count: usize,
    /// Entries per bucket, i.e. how many colliding keys coexist. Raise it when
    /// a burst of unimportant keys would otherwise push important ones out.
    bucket_size: u8,
};

/// `Context` supplies the key policy and must declare:
///
///   - `fn hash(*const Context, K) u64`
///   - `fn eql(*const Context, K, K) bool`
///
/// and may optionally declare `fn evicted(*const Context, K, V) void`, called
/// whenever an entry leaves the table by eviction or `clear`.
pub fn CacheTable(
    comptime K: type,
    comptime V: type,
    comptime Context: type,
    comptime opts: Options,
) type {
    return struct {
        const Self = @This();

        comptime {
            std.debug.assert(std.math.isPowerOfTwo(opts.bucket_count));
            std.debug.assert(opts.bucket_size > 0);
        }

        const INDEX_MASK: usize = opts.bucket_count - 1;

        pub const Entry = struct {
            key: K,
            value: V,
            /// Absolute deadline. The entry is dead once `now_ms >= expires_at_ms`.
            expires_at_ms: i64,
        };

        buckets: [opts.bucket_count][opts.bucket_size]Entry,
        lengths: [opts.bucket_count]u8,
        context: Context,

        pub fn init(context: Context) Self {
            return .{
                // SAFETY: a slot is only ever read below its bucket's recorded
                // length, and every length starts at zero, so no read can reach
                // an entry that was never written. Zeroing the storage would
                // cost a table-sized memset to establish what no read depends
                // on, and `Entry` has no meaningful zero for a generic K/V.
                .buckets = undefined,
                .lengths = @splat(0),
                .context = context,
            };
        }

        /// The stored value for `key`, or null when absent or expired.
        ///
        /// Does not mutate, so it is safe to call under a shared/read lock. It
        /// therefore does not refresh LRU position — a consumer whose hot keys
        /// must survive eviction pressure wants `get`.
        ///
        /// `now_ms` is a parameter rather than a clock read so expiry boundaries
        /// are provable without sleeping.
        pub fn peek(self: *const Self, key: K, now_ms: i64) ?V {
            const idx = self.bucketIndex(key);
            const len = self.lengths[idx];
            var i: usize = len;
            while (i > 0) {
                i -= 1;
                const entry = &self.buckets[idx][i];
                if (!self.context.eql(key, entry.key)) continue;
                if (now_ms >= entry.expires_at_ms) return null;
                return entry.value;
            }
            return null;
        }

        /// The stored value for `key`, promoting it to most-recently-used.
        ///
        /// Mutates, so it needs exclusive access. An expired entry is dropped on
        /// the way past rather than left to be re-read.
        pub fn get(self: *Self, key: K, now_ms: i64) ?V {
            const idx = self.bucketIndex(key);
            const len = self.lengths[idx];
            var i: usize = len;
            while (i > 0) {
                i -= 1;
                if (!self.context.eql(key, self.buckets[idx][i].key)) continue;
                if (now_ms >= self.buckets[idx][i].expires_at_ms) {
                    self.removeAt(idx, i);
                    return null;
                }
                const value = self.buckets[idx][i].value;
                rotateOnce(self.buckets[idx][i..len]);
                return value;
            }
            return null;
        }

        /// Store `key`'s value until `expires_at_ms`, returning any live entry
        /// evicted to make room.
        ///
        /// Reuses the key's own entry, then any expired one, before evicting
        /// anything live — so an expired entry never costs a live one its slot.
        ///
        /// The displaced entry — whether it was this key's previous value, an
        /// expired neighbour, or the evicted least-recently-used one — is passed
        /// to `Context.evicted` if declared. A returned entry has therefore
        /// already been released: read its `key`, never its `value`.
        pub fn put(self: *Self, key: K, value: V, expires_at_ms: i64, now_ms: i64) ?Entry {
            const idx = self.bucketIndex(key);
            const len = self.lengths[idx];
            const entry: Entry = .{ .key = key, .value = value, .expires_at_ms = expires_at_ms };

            for (self.buckets[idx][0..len], 0..) |*slot, i| {
                const reusable = self.context.eql(key, slot.key) or now_ms >= slot.expires_at_ms;
                if (!reusable) continue;
                // The occupant is dropped here, not overwritten silently: for a
                // value that owns memory, refreshing a key would otherwise leak
                // its previous body on every single write.
                self.release(slot.*);
                slot.* = entry;
                rotateOnce(self.buckets[idx][i..len]);
                return null;
            }

            if (len < opts.bucket_size) {
                self.buckets[idx][len] = entry;
                self.lengths[idx] = len + 1;
                return null;
            }

            const evicted = rotateIn(&self.buckets[idx], entry);
            self.release(evicted);
            return evicted;
        }

        /// Drop `key`'s entry. True when one was present, expired or not.
        pub fn remove(self: *Self, key: K) bool {
            const idx = self.bucketIndex(key);
            var i: usize = self.lengths[idx];
            while (i > 0) {
                i -= 1;
                if (!self.context.eql(key, self.buckets[idx][i].key)) continue;
                self.removeAt(idx, i);
                return true;
            }
            return false;
        }

        /// Drop every entry `pred.match(key, value)` accepts; returns the count.
        ///
        /// Walks the whole table, so it belongs on control-plane actions rather
        /// than a request path. It exists for invalidation keyed by something
        /// other than the cache key — a caller holding an identity that maps to
        /// entries it cannot name.
        pub fn removeMatching(self: *Self, pred: anytype) usize {
            var removed: usize = 0;
            for (0..opts.bucket_count) |idx| {
                var i: usize = self.lengths[idx];
                while (i > 0) {
                    i -= 1;
                    const entry = &self.buckets[idx][i];
                    if (!pred.match(entry.key, entry.value)) continue;
                    self.removeAt(idx, i);
                    removed += 1;
                }
            }
            return removed;
        }

        /// Drop everything. Fires `Context.evicted` for each entry if declared.
        pub fn clear(self: *Self) void {
            for (self.buckets, self.lengths) |bucket, len| {
                for (bucket[0..len]) |entry| self.release(entry);
            }
            self.lengths = @splat(0);
        }

        /// Live (unexpired) entries. Walks the table — tests and gauges only.
        pub fn count(self: *const Self, now_ms: i64) usize {
            var live: usize = 0;
            for (self.buckets, self.lengths) |bucket, len| {
                for (bucket[0..len]) |entry| {
                    if (now_ms < entry.expires_at_ms) live += 1;
                }
            }
            return live;
        }

        fn bucketIndex(self: *const Self, key: K) usize {
            return @intCast(self.context.hash(key) & INDEX_MASK);
        }

        /// Compacts the entry at `i` out of its bucket, preserving LRU order of
        /// the rest. Order matters: a swap-remove would promote whatever it
        /// moved into the gap.
        fn removeAt(self: *Self, idx: usize, i: usize) void {
            const len = self.lengths[idx];
            self.release(self.buckets[idx][i]);
            std.mem.copyForwards(
                Entry,
                self.buckets[idx][i .. len - 1],
                self.buckets[idx][i + 1 .. len],
            );
            self.lengths[idx] = len - 1;
        }

        /// The single exit an entry can leave by. Every drop, overwrite, and
        /// eviction routes here, so a `Context` whose values own heap memory has
        /// exactly one place to free them and cannot leak through a path the
        /// table forgot to wire up.
        fn release(self: *Self, entry: Entry) void {
            if (comptime @hasDecl(Context, EVICTION_HOOK)) {
                self.context.evicted(entry.key, entry.value);
            }
        }
    };
}

/// Moves the first item to the end: `0 1 2 3` -> `1 2 3 0`.
fn rotateOnce(items: anytype) void {
    if (items.len <= 1) return;
    const tmp = items[0];
    std.mem.copyForwards(@TypeOf(tmp), items[0 .. items.len - 1], items[1..]);
    items[items.len - 1] = tmp;
}

/// Rotates `item` in at the end and returns the displaced first item.
fn rotateIn(items: anytype, item: anytype) @TypeOf(item) {
    const removed = items[0];
    std.mem.copyForwards(@TypeOf(item), items[0 .. items.len - 1], items[1..]);
    items[items.len - 1] = item;
    return removed;
}

test {
    _ = @import("cache_table_test.zig");
}
