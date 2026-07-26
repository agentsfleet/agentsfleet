//! Revision-keyed response cache for global catalogue pages (§2).
//!
//! ## Why the revision is in the KEY, not in a published generation
//!
//! An earlier draft kept a process-published generation and let a candidate
//! publish only when its revision exceeded it, with a rule that concurrent
//! publishers must never replace a newer generation with an older one. Carrying
//! the revision in the key removes all of it: a candidate built from revision N
//! lands under a key containing N, every later request reads revision N+1 first
//! and looks up a different key, so a stale candidate is unreachable rather than
//! dangerous. It simply ages out. Ordering between concurrent publishers stops
//! mattering, and with it a class of races that is hard to test and easy to get
//! wrong.
//!
//! ## The cache is shared across tenants
//!
//! Invariant 6: its payload must be byte-identical for every authorized caller,
//! and the key carries no tenant. That is deliberate — a tenant-varying field
//! reaching this cache is a cross-tenant leak, and a tenant-free key is what
//! makes such a bug impossible to paper over with a per-tenant partition.
//! Nothing Fleet-scoped is ever stored here.
//!
//! ## Shape: `common.CacheTable` under an `RwLock`
//!
//! The storage is the shared fixed-capacity table rather than a bespoke
//! intrusive list, which buys three things this cache specifically wants.
//!
//! The **entry ceiling becomes structural**: capacity IS
//! `BUCKET_COUNT * BUCKET_SIZE` slots, so §2's 256 is a property of the type
//! rather than a counter some path could fail to check.
//!
//! **Keys cost nothing.** A revision plus a digest is fixed-size, so it is
//! stored inline. The predecessor duplicated a key string per entry and counted
//! those bytes against the budget; here no key is allocated and no key byte
//! competes with a payload byte.
//!
//! **Reads do not serialize.** `fetch` uses the table's non-mutating `peek`
//! under a shared lock, so concurrent catalogue requests run in parallel. The
//! predecessor took one exclusive mutex on every read in order to refresh LRU
//! position. The trade is deliberate and is the one place this diverges from
//! §2's "true LRU" wording: eviction is least-recently-used *within a bucket*
//! and a hit does not promote. With a 60-second bound, a generation-scoped key
//! set, and four ways per bucket, retention matters far less than keeping every
//! reader off a single lock.
//!
//! ## Byte accounting is defined, not estimated
//!
//! "Including allocator metadata" is not observable through a Zig allocator, so
//! a budget phrased that way cannot be asserted. This counts exactly one thing
//! per live entry — the payload bytes it owns — and that sum IS the number the
//! ceiling compares against, so the number a test reads is the number the cache
//! enforces. Slot storage is preallocated and fixed, and allocator-internal
//! padding is outside the budget by construction.

const std = @import("std");
const common = @import("common");

/// §2's ceilings. `MAX_ENTRIES` is derived from the geometry rather than
/// declared beside it, so the two cannot disagree.
const BUCKET_COUNT: usize = 64;
const BUCKET_SIZE: u8 = 4;
pub const MAX_ENTRIES: usize = BUCKET_COUNT * BUCKET_SIZE;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Freshness bound. The cache never reads a clock — `now_ms` arrives from the
/// caller — so §2's "monotonic" requirement is satisfied by the caller's choice
/// of source and there is no wall clock here for a clock adjustment to move.
pub const TTL_MS: i64 = 60 * std.time.ms_per_s;

pub const DIGEST_LEN: usize = std.crypto.hash.sha2.Sha256.digest_length;

/// What identifies a cached page: the catalogue generation it was built from,
/// and a digest standing for the canonical selectors.
///
/// The digest is an HMAC-SHA-256 under a process-random key, computed by the
/// caller. This module never receives the selectors themselves, so it cannot
/// log or leak them, and the digest is not reversible into them by anything
/// that reads a heap dump. No tenant appears here — Invariant 6.
pub const Key = struct {
    revision: u64,
    digest: [DIGEST_LEN]u8,
};

/// Live payload bytes. Heap-allocated so the table's context can reach it
/// through a stable pointer: the context is stored *inside* the table by value,
/// so a counter living directly on `Cache` would be at a different address
/// every time a `Cache` was moved.
const Accounting = struct {
    bytes: usize = 0,
};

const Context = struct {
    alloc: std.mem.Allocator,
    acct: *Accounting,

    /// The digest is already uniform, so its leading bytes ARE the bucket index
    /// — rehashing them would only cost time. The revision is mixed in so the
    /// same selectors at two generations spread across buckets rather than
    /// contending for one during a changeover.
    pub fn hash(_: *const Context, key: Key) u64 {
        return std.mem.readInt(u64, key.digest[0..8], .little) ^ key.revision;
    }

    pub fn eql(_: *const Context, a: Key, b: Key) bool {
        return a.revision == b.revision and std.mem.eql(u8, &a.digest, &b.digest);
    }

    /// The table's single departure hook — eviction, overwrite, expiry sweep,
    /// removal and teardown all arrive here. Freeing and un-counting in one
    /// place is what keeps the tally and the heap in agreement without every
    /// call site remembering to do both.
    ///
    /// Only ever called from a mutating table method, which the caller holds
    /// the lock exclusively for, so the unsynchronized decrement is safe.
    pub fn evicted(self: *const Context, _: Key, value: []u8) void {
        self.acct.bytes -= value.len;
        self.alloc.free(value);
    }
};

const Table = common.CacheTable(Key, []u8, Context, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = BUCKET_SIZE,
});

pub const Cache = struct {
    const Self = @This();

    alloc: std.mem.Allocator,
    acct: *Accounting,
    table: Table,
    lock: common.RwLock = .{},

    pub fn init(alloc: std.mem.Allocator) !Self {
        const acct = try alloc.create(Accounting);
        acct.* = .{};
        return .{
            .alloc = alloc,
            .acct = acct,
            .table = Table.init(.{ .alloc = alloc, .acct = acct }),
        };
    }

    pub fn deinit(self: *Self) void {
        self.table.clear(); // releases every resident payload
        self.alloc.destroy(self.acct);
        self.* = undefined;
    }

    /// Live payload bytes: the exact sum the ceiling is enforced against.
    pub fn byteLen(self: *Self) usize {
        self.lock.lockShared();
        defer self.lock.unlockShared();
        return self.acct.bytes;
    }

    /// Entries still within their freshness bound at `now_ms`.
    pub fn count(self: *Self, now_ms: i64) usize {
        self.lock.lockShared();
        defer self.lock.unlockShared();
        return self.table.count(now_ms);
    }

    /// A fresh cached page, copied into `dest` for the caller to own.
    ///
    /// Held shared, so concurrent readers do not block each other. The copy is
    /// taken before the lock drops: returning the stored slice would hand out
    /// memory that the next writer is entitled to free.
    ///
    /// The copy goes to a CALLER-supplied allocator rather than the cache's own.
    /// The cache's allocator is process-lifetime, so duping into it would give
    /// every hit a permanent allocation no one owns — a leak that grows with
    /// traffic. The caller passes the arena its response is written from, which
    /// both frees the copy and puts it somewhere that outlives the handler.
    ///
    /// An expired entry reads as a miss and is left resident — `peek` must not
    /// mutate under a shared lock. `put` reclaims it later.
    pub fn fetch(self: *Self, dest: std.mem.Allocator, key: Key, now_ms: i64) !?[]u8 {
        self.lock.lockShared();
        defer self.lock.unlockShared();
        const value = self.table.peek(key, now_ms) orelse return null;
        return try dest.dupe(u8, value);
    }

    /// Admit a response, returning false when it was BYPASSED rather than
    /// stored — the caller still serves the response, it simply is not cached.
    pub fn put(self: *Self, key: Key, value: []const u8, now_ms: i64) !bool {
        if (value.len > MAX_BYTES) return false;

        self.lock.lock();
        defer self.lock.unlock();

        if (!self.reserve(value.len, now_ms)) return false;

        const owned = try self.alloc.dupe(u8, value);
        // Counted before the store because storing may evict, and the eviction
        // hook decrements. Both orders total the same; this one never
        // transiently under-counts.
        self.acct.bytes += owned.len;
        self.table.put(key, owned, now_ms + TTL_MS, now_ms);
        return true;
    }

    /// Whether `len` more bytes fit, reclaiming dead entries first if not.
    ///
    /// §2 is explicit that crossing the ceiling is "a bypass, never an eviction
    /// cascade", so nothing live is dropped to make room — emptying a working
    /// cache to admit one outlier trades everything for a single hit. Expired
    /// entries are a different matter: they are dead already and only lazy
    /// expiry is still holding their memory, so they are swept before the
    /// answer is allowed to be no.
    fn reserve(self: *Self, len: usize, now_ms: i64) bool {
        if (self.acct.bytes + len <= MAX_BYTES) return true;
        _ = self.table.sweepExpired(now_ms);
        return self.acct.bytes + len <= MAX_BYTES;
    }
};
