//! Fixed-capacity associative cache for values that can be recomputed.
//!
//! `bucket_count` buckets of `bucket_size` slots, in one flat array, allocated
//! once and never grown. A key's hash picks its bucket and it can live in no
//! other, so a lookup is at most `bucket_size` comparisons and no operation ever
//! allocates. Inserting into a full bucket drops that bucket's oldest entry.
//!
//! ## What this deliberately does NOT have
//!
//! No expiry, no `now_ms`, no removal by key. Those were here once and each one
//! added a way for an entry to leave, which is the only thing this structure can
//! get wrong: the release hook has to cover every exit, and a key that can be
//! removed must not be resident twice. With only the two exits below, both
//! properties hold by construction rather than by care.
//!
//! Staleness is therefore the consumer's problem, and both consumers already
//! solve it without a clock: `state/model_library_cache.zig` puts the catalogue
//! revision IN THE KEY, so an entry built from an older revision is unreachable
//! rather than wrong, and `state/model_rate_cache.zig` puts the generation in
//! the VALUE, so a reader accepts an entry only at the generation it observed.
//! A cache of recomputable values may forget anything at any time; it may never
//! answer with something else's value.
//!
//! ## The two exits, and the release rule
//!
//! An entry leaves only by (1) losing its slot to a newer entry in a full
//! bucket, or (2) `clear`. Both call `Context.evicted` if it is declared, so a
//! consumer whose `K` or `V` owns heap memory frees it in exactly one place and
//! cannot leak through a path this file forgot to wire up.
//!
//! ## Duplicate keys are allowed, and the newest wins
//!
//! `put` does not look for the key first — it appends. Two entries for one key
//! can therefore coexist, and `peek` scans a bucket from its newest end, so it
//! finds the most recent one and the older is unreachable until evicted. That is
//! safe here because both consumers `put` only after a miss and neither can
//! remove by key; the wasted slot is bounded by `bucket_size`. Do not add a
//! removal verb without also making `put` reuse the key's own slot — a removal
//! that drops one of two entries for a key resurrects the other.
//!
//! ## Attribution
//!
//! The bucket-and-lengths layout, eviction by rotating the newcomer in, and the
//! optional eviction hook are adapted from Ghostty's
//! `src/datastruct/cache_table.zig` — MIT, Copyright (c) 2024 Mitchell
//! Hashimoto, Ghostty contributors. Ghostty's `get` refreshes recency on a hit;
//! neither consumer here reads through a mutating path, so that is omitted and
//! eviction is oldest-inserted within the bucket.
//!
//! Deriving the bucket index from a digest's leading bytes rather than re-hashing
//! an already-uniform key is from Bun's `src/runtime/api/bun/SSLContextCache.zig`
//! — MIT, Copyright (c) Oven. Applied by consumers in their `Context.hash`.

const std = @import("std");

/// Optional `Context` declaration called whenever an entry leaves the table.
const EVICTION_HOOK = "evicted";

pub const Options = struct {
    /// Number of buckets. Power of two — the index is a mask, not a modulo.
    /// Size it near the count of keys expected live at once.
    bucket_count: usize,
    /// Slots per bucket. Larger tolerates more keys colliding on one bucket
    /// before they start evicting each other.
    bucket_size: u8,
};

/// `Context` must declare:
///   - `fn hash(*const Context, K) u64`
///   - `fn eql(*const Context, K, K) bool`
///
/// and may optionally declare `fn evicted(*const Context, K, V) void`, called
/// once for every entry that leaves the table (see §The two exits above).
pub fn CacheTable(
    comptime K: type,
    comptime V: type,
    comptime Context: type,
    comptime opts: Options,
) type {
    return struct {
        const Self = @This();
        const INDEX_MASK: u64 = opts.bucket_count - 1;

        comptime {
            if (!std.math.isPowerOfTwo(opts.bucket_count))
                @compileError("bucket_count must be a power of two — the index is a mask");
            if (opts.bucket_size == 0)
                @compileError("bucket_size must be at least 1");
        }

        const Entry = struct {
            key: K,
            value: V,
        };

        buckets: [opts.bucket_count][opts.bucket_size]Entry,
        lengths: [opts.bucket_count]u8,
        context: Context,

        pub fn init(context: Context) Self {
            // SAFETY: a slot is only ever read below its bucket's recorded
            // length, and reaching that length means a real entry was written
            // there — `lengths` starts at zero, so nothing is readable until a
            // `put` writes it. Zeroing `buckets` would cost a megabyte-scale
            // memset at startup to establish something no read depends on, and
            // `Entry` has no meaningful zero for a generic K/V.
            return .{ .buckets = undefined, .lengths = @splat(0), .context = context };
        }

        /// The stored value for `key`, or null when absent.
        ///
        /// Does not mutate, so it is safe under a shared/read lock — which is
        /// why both consumers can serve concurrent readers without serializing
        /// on one exclusive lock. Scans from the bucket's newest end, so when a
        /// key is resident twice this is the most recently stored one.
        pub fn peek(self: *const Self, key: K) ?V {
            const idx = self.bucketIndex(key);
            var i: usize = self.lengths[idx];
            while (i > 0) {
                i -= 1;
                const entry = &self.buckets[idx][i];
                if (self.context.eql(key, entry.key)) return entry.value;
            }
            return null;
        }

        /// Store `key`'s value, evicting this bucket's oldest entry if it is
        /// full. The evicted entry is passed to `Context.evicted` if declared.
        ///
        /// Appends without looking for `key` first: both consumers call this
        /// only after a miss, so the duplicate case is rare, bounded, and
        /// answered correctly by `peek` (see §Duplicate keys above).
        pub fn put(self: *Self, key: K, value: V) void {
            const idx = self.bucketIndex(key);
            const len = self.lengths[idx];
            const entry: Entry = .{ .key = key, .value = value };

            if (len < opts.bucket_size) {
                self.buckets[idx][len] = entry;
                self.lengths[idx] = len + 1;
                return;
            }
            self.release(rotateIn(&self.buckets[idx], entry));
        }

        /// Drop everything, releasing each resident entry.
        pub fn clear(self: *Self) void {
            for (&self.buckets, self.lengths) |*bucket, len| {
                for (bucket[0..len]) |entry| self.release(entry);
            }
            self.lengths = @splat(0);
        }

        /// Resident entries. Walks the lengths array — gauges and tests only.
        pub fn count(self: *const Self) usize {
            var live: usize = 0;
            for (self.lengths) |len| live += len;
            return live;
        }

        fn bucketIndex(self: *const Self, key: K) usize {
            return @intCast(self.context.hash(key) & INDEX_MASK);
        }

        /// The single exit an entry can leave by. Both eviction and `clear`
        /// route here, so a `Context` whose entries own heap memory has exactly
        /// one place to free them.
        fn release(self: *Self, entry: Entry) void {
            if (comptime @hasDecl(Context, EVICTION_HOOK)) {
                self.context.evicted(entry.key, entry.value);
            }
        }

        /// Rotates `item` in at the end and returns the displaced first item.
        fn rotateIn(bucket: *[opts.bucket_size]Entry, item: Entry) Entry {
            const removed = bucket[0];
            std.mem.copyForwards(Entry, bucket[0 .. bucket.len - 1], bucket[1..]);
            bucket[bucket.len - 1] = item;
            return removed;
        }
    };
}

test {
    _ = @import("cache_table_test.zig");
}
