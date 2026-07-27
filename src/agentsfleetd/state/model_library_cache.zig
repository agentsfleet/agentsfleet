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
//! §2's "true LRU" wording: eviction drops a bucket's oldest entry and a hit
//! does not promote. With a generation-scoped key set — every resident entry is
//! current-revision, because a bump clears the table — and four ways per bucket,
//! retention matters far less than keeping every reader off a single lock.
//!
//! ## Nothing here reads a clock
//!
//! An earlier shape gave each entry a 60-second deadline. It was never a
//! freshness bound: the revision in the key already makes a superseded page
//! unreachable. Its real job was reclaiming those unreachable payloads before
//! they starved the byte ceiling — which `clear` on a revision bump does exactly,
//! immediately, and without a deadline to tune — the bump path in
//! `state/model_catalogue_revision.zig` is what changes the key, and the entries
//! under the old one age out under bucket pressure.
//!
//! ## Byte accounting is defined, not estimated
//!
//! The bound is the SLOT COUNT, and it is the only bound.
//!
//! §2 also named an 8 MiB byte ceiling, enforced by a running total with a bypass
//! above it. That total is gone, because the geometry already binds well under
//! it. A `model_library_store.LibraryRow` is six fields — id, provider, context
//! cap, three prices — and serializes to 188 bytes at the median of the shipped
//! fixture ids (180–217). A full `limit` = 50 page is therefore ≈9.2 KB, and 256
//! slots of full pages ≈2.3 MiB: under a third of the ceiling, so the byte total
//! could never fire.
//!
//! Keeping it would have cost a reclamation policy for no bound at all. With no
//! clock in the table, nothing drops the superseded payloads still holding
//! budget, so a byte ceiling would eventually wedge — resident on stale pages,
//! bypassing every new one — which is what the deleted expiry sweep existed to
//! prevent. Removing the ceiling removes the need for the sweep.
//!
//! So memory here is `MAX_ENTRIES` × page size. The tripwire if a row ever grows
//! is `observability/library_read_counters.GLOBAL_MODELS_MAX_BODY_BYTES`
//! (256 KiB), which the read-bounds suites assert every page against — at that
//! ceiling 256 slots would be 64 MiB, so a row shape that grows 27× is the point
//! at which this reasoning needs redoing.
//!
//! ## No recency, on purpose
//!
//! The table evicts a bucket's oldest INSERTED entry; a read does not promote.
//! Ghostty's original refreshes recency on a hit, which retains hot entries
//! better — but that read mutates, so it needs an exclusive lock, and every
//! catalogue request would serialize behind one mutex. The trade is right only
//! because this table is heavily over-provisioned against its working set: 256
//! slots for the handful of query shapes that are hot at one revision, so
//! eviction essentially never fires and retention is moot. If that stops being
//! true, the fix is Ghostty's promoting read and an exclusive lock — a decision,
//! not a rediscovery.

const std = @import("std");
const common = @import("common");

/// §2's entry ceiling, derived from the geometry rather than declared beside it,
/// so the two cannot disagree. It is the cache's only bound (see the module note).
const BUCKET_COUNT: usize = 64;
const BUCKET_SIZE: u8 = 4;
pub const MAX_ENTRIES: usize = BUCKET_COUNT * BUCKET_SIZE;

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

const Context = struct {
    alloc: std.mem.Allocator,

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

    /// The table's single departure hook — bucket eviction and `clear` are its
    /// only two exits, and both arrive here, so a payload is freed in exactly one
    /// place and cannot leak through a path the table forgot to wire up.
    pub fn evicted(self: *const Context, _: Key, value: []u8) void {
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
    table: Table,
    lock: common.RwLock = .{},

    pub fn init(alloc: std.mem.Allocator) Self {
        return .{ .alloc = alloc, .table = Table.init(.{ .alloc = alloc }) };
    }

    pub fn deinit(self: *Self) void {
        self.table.clear(); // releases every resident payload
        self.* = undefined;
    }

    /// Resident entries. All of them are current-revision (see `put`).
    pub fn count(self: *Self) usize {
        self.lock.lockShared();
        defer self.lock.unlockShared();
        return self.table.count();
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
    /// A page cached under a superseded revision cannot be reached from here at
    /// all: the revision is part of the key, so a caller that has observed a
    /// newer one looks up a different key and misses.
    pub fn fetch(self: *Self, dest: std.mem.Allocator, key: Key) !?[]u8 {
        self.lock.lockShared();
        defer self.lock.unlockShared();
        const value = self.table.peek(key) orelse return null;
        return try dest.dupe(u8, value);
    }

    /// Admit a response. The only failure is an allocation fault; the caller
    /// serves the page either way, so a refusal is a non-event.
    ///
    /// Admission is unconditional because the geometry is the bound: this may
    /// displace a page from the key's bucket, and `evicted` frees it.
    pub fn put(self: *Self, key: Key, value: []const u8) !void {
        const owned = try self.alloc.dupe(u8, value);
        self.lock.lock();
        defer self.lock.unlock();
        self.table.put(key, owned);
    }
};
