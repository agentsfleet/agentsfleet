//! Revision-keyed LRU for global catalogue responses (§2).
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
//! ## Byte accounting is defined, not estimated
//!
//! "Including allocator metadata" is not observable through a Zig allocator, so
//! a budget phrased that way cannot be asserted. This counts exactly three
//! things per live entry — key bytes, value bytes, and node storage — and that
//! sum IS the number the ceiling compares against, so the number the test reads
//! is the number the cache enforces. Allocator-internal padding is outside the
//! budget by construction, and `MAX_BYTES` is set with that headroom in mind.
//!
//! ## The cache is shared across tenants
//!
//! Invariant 6: its payload must be byte-identical for every authorized caller,
//! and the key carries no tenant. That is deliberate — a tenant-varying field
//! reaching this cache is a cross-tenant leak, and a tenant-free key is what
//! makes such a bug impossible to paper over with a per-tenant partition.
//! Nothing Fleet-scoped is ever stored here.

const std = @import("std");
const common = @import("common");

/// Ceilings from §2. Entries first: a page is bounded to 100 items, so 256
/// distinct selector combinations is already generous for a catalogue that
/// changes rarely.
pub const MAX_ENTRIES: usize = 256;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;

/// Freshness bound, monotonic. Wall-clock would let a clock adjustment either
/// resurrect an expired entry or expire a live one; neither is acceptable for a
/// value a caller may act on.
pub const TTL_NANOS: u64 = 60 * std.time.ns_per_s;

const Entry = struct {
    key: []u8,
    value: []u8,
    stored_at: u64,
    /// LRU order, most-recently-used first.
    newer: ?*Entry = null,
    older: ?*Entry = null,
};

/// Per-entry overhead counted against `MAX_BYTES` alongside the key and value
/// bytes. Named so the accounting is auditable rather than a magic addend.
const NODE_BYTES: usize = @sizeOf(Entry);

pub const Cache = struct {
    const Self = @This();
    const Map = std.StringHashMapUnmanaged(*Entry);

    alloc: std.mem.Allocator,
    map: Map = .{},
    /// Most- and least-recently-used ends of the intrusive list.
    mru: ?*Entry = null,
    lru: ?*Entry = null,
    bytes: usize = 0,
    lock: common.Mutex = .{},

    pub fn init(alloc: std.mem.Allocator) Self {
        return .{ .alloc = alloc };
    }

    pub fn deinit(self: *Self) void {
        var it = self.map.iterator();
        while (it.next()) |e| self.destroy(e.value_ptr.*);
        self.map.deinit(self.alloc);
        self.* = undefined;
    }

    fn destroy(self: *Self, e: *Entry) void {
        self.alloc.free(e.key);
        self.alloc.free(e.value);
        self.alloc.destroy(e);
    }

    /// Live bytes: the exact sum this cache's ceiling is enforced against.
    pub fn byteLen(self: *Self) usize {
        self.lock.lock();
        defer self.lock.unlock();
        return self.bytes;
    }

    pub fn count(self: *Self) usize {
        self.lock.lock();
        defer self.lock.unlock();
        return self.map.count();
    }

    fn unlink(self: *Self, e: *Entry) void {
        if (e.newer) |n| n.older = e.older else self.mru = e.older;
        if (e.older) |o| o.newer = e.newer else self.lru = e.newer;
        e.newer = null;
        e.older = null;
    }

    fn pushFront(self: *Self, e: *Entry) void {
        e.older = self.mru;
        e.newer = null;
        if (self.mru) |m| m.newer = e;
        self.mru = e;
        if (self.lru == null) self.lru = e;
    }

    fn evictLru(self: *Self) void {
        const victim = self.lru orelse return;
        self.unlink(victim);
        _ = self.map.remove(victim.key);
        self.bytes -= victim.key.len + victim.value.len + NODE_BYTES;
        self.destroy(victim);
    }

    /// Look up a fresh entry, promoting it to most-recently-used.
    ///
    /// An expired entry is removed rather than returned — a stale hit is worse
    /// than a miss, because the caller cannot tell the difference. Caller owns
    /// the returned copy.
    pub fn get(self: *Self, key: []const u8, now: u64) !?[]u8 {
        self.lock.lock();
        defer self.lock.unlock();

        const e = self.map.get(key) orelse return null;
        if (now -% e.stored_at >= TTL_NANOS) {
            self.unlink(e);
            _ = self.map.remove(e.key);
            self.bytes -= e.key.len + e.value.len + NODE_BYTES;
            self.destroy(e);
            return null;
        }
        self.unlink(e);
        self.pushFront(e);
        return try self.alloc.dupe(u8, e.value);
    }

    /// Admit a response. Returns false when the entry was BYPASSED rather than
    /// stored — the caller still serves the response, it simply is not cached.
    ///
    /// An entry whose own footprint exceeds `MAX_BYTES` is bypassed outright
    /// rather than emptying the cache to make room for it: evicting every useful
    /// entry to admit one oversized outlier trades a working cache for a single
    /// hit. Ordinary pressure still evicts least-recently-used, which is what
    /// makes this an LRU rather than a fill-once buffer.
    pub fn put(self: *Self, key: []const u8, value: []const u8, now: u64) !bool {
        const footprint = key.len + value.len + NODE_BYTES;
        if (footprint > MAX_BYTES) return false;

        self.lock.lock();
        defer self.lock.unlock();

        // Replacing an existing key must not double-count it.
        if (self.map.get(key)) |old| {
            self.unlink(old);
            _ = self.map.remove(old.key);
            self.bytes -= old.key.len + old.value.len + NODE_BYTES;
            self.destroy(old);
        }

        while (self.map.count() + 1 > MAX_ENTRIES or self.bytes + footprint > MAX_BYTES) {
            if (self.lru == null) break;
            self.evictLru();
        }
        // Nothing left to evict and it still does not fit.
        if (self.bytes + footprint > MAX_BYTES) return false;

        const e = try self.alloc.create(Entry);
        errdefer self.alloc.destroy(e);
        const k = try self.alloc.dupe(u8, key);
        errdefer self.alloc.free(k);
        const v = try self.alloc.dupe(u8, value);
        errdefer self.alloc.free(v);

        e.* = .{ .key = k, .value = v, .stored_at = now };
        try self.map.put(self.alloc, k, e);
        self.pushFront(e);
        self.bytes += footprint;
        return true;
    }

    /// Test-only: the key of the least-recently-used entry, so eviction ORDER is
    /// assertable rather than inferred from which lookups happen to miss.
    pub fn lruKeyForTest(self: *Self) ?[]const u8 {
        self.lock.lock();
        defer self.lock.unlock();
        return if (self.lru) |l| l.key else null;
    }
};
