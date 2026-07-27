//! Per-process memo of which fleets already have their `fleet_lease` consumer
//! group created.
//!
//! `XGROUP CREATE … MKSTREAM` is idempotent, so the unmemoized form was correct
//! but paid one Redis round-trip per candidate per lease poll, using the
//! `BUSYGROUP` error reply as its steady state. The group is durable: once
//! created it stays created, so the answer is memoizable for the process
//! lifetime.
//!
//! **A miss is cheap; a wrong hit is not.** Every way this table can forget
//! costs exactly one redundant Redis command:
//!
//!   - A bucket overflows and evicts an entry: the evicted fleet issues a real
//!     `XGROUP CREATE` on its next poll.
//!   - Two threads record concurrently: both write the same value.
//!   - A group is deleted out of band: the following stream read fails,
//!     `invalidate` clears the entry, and the next poll creates it for real.
//!
//! What it must never do is report a fleet ensured when it is not, because that
//! skips a genuinely needed create. The predecessor stored a 64-bit hash with no
//! key and so could do exactly that on a hash collision; entries here carry the
//! fleet id and are compared in full, which retires that case.
//!
//! **Shape.** `common.CacheTable` under an `RwLock`. `isEnsured` is on the
//! publish path and never mutates, so it takes the lock shared and readers do
//! not serialize against each other; the two mutating calls run on create and on
//! read-failure paths that already involve a Redis round-trip.

const std = @import("std");
const common = @import("common");

/// Buckets, each holding `ENTRIES_PER_BUCKET` fleets.
const BUCKET_COUNT: usize = 1024;
const ENTRIES_PER_BUCKET: u8 = 4;

/// Distinct fleets tracked per process before eviction begins.
pub const CAPACITY: usize = BUCKET_COUNT * ENTRIES_PER_BUCKET;

/// Canonical UUID text — the widest `fleet_id` a row can carry. A longer id is
/// simply not memoized (see `asKey`), which costs a redundant command and never
/// a wrong answer.
const FLEET_ID_MAX_LEN: usize = 36;

/// The memo has no time dimension — a created consumer group stays created, so
/// entries are stored with `NEVER_EXPIRES` and every read is asked "as of" the
/// same fixed instant.
const TIMELESS: i64 = 0;

const FleetKey = struct {
    buf: [FLEET_ID_MAX_LEN]u8,
    len: usize,

    fn id(self: *const FleetKey) []const u8 {
        return self.buf[0..self.len];
    }
};

const FleetContext = struct {
    /// Hashed rather than sliced from the id's leading bytes. A fleet id is
    /// UUIDv7, whose leading bytes are a millisecond timestamp — fleets created
    /// in the same millisecond share that prefix, so using it as the bucket
    /// index would pile a burst of new fleets into one bucket. (That shortcut is
    /// valid only for uniformly distributed keys, e.g. a digest.)
    pub fn hash(_: *const FleetContext, key: FleetKey) u64 {
        var h = std.hash.Wyhash.init(0);
        h.update(key.id());
        return h.final();
    }

    pub fn eql(_: *const FleetContext, a: FleetKey, b: FleetKey) bool {
        return std.mem.eql(u8, a.id(), b.id());
    }
};

const MemoTable = common.CacheTable(FleetKey, void, FleetContext, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = ENTRIES_PER_BUCKET,
});

var g_lock: common.RwLock = .{};
var g_table: MemoTable = MemoTable.init(.{});

fn asKey(fleet_id: []const u8) ?FleetKey {
    if (fleet_id.len == 0 or fleet_id.len > FLEET_ID_MAX_LEN) return null;
    var key: FleetKey = .{ .buf = @splat(0), .len = fleet_id.len };
    @memcpy(key.buf[0..fleet_id.len], fleet_id);
    return key;
}

/// True when this process has already created `fleet_id`'s consumer group and
/// nothing has invalidated that. A false answer costs one Redis command.
pub fn isEnsured(fleet_id: []const u8) bool {
    const key = asKey(fleet_id) orelse return false;
    g_lock.lockShared();
    defer g_lock.unlockShared();
    // `peek` is the non-mutating reader, which is what makes the shared lock
    // sound; it forgoes refreshing eviction order, and this table never expires.
    return g_table.peek(key, TIMELESS) != null;
}

/// Record that `fleet_id`'s group exists. Call only after a create that returned
/// OK or reported BUSYGROUP — both prove the group is present.
pub fn recordEnsured(fleet_id: []const u8) void {
    const key = asKey(fleet_id) orelse return;
    g_lock.lock();
    defer g_lock.unlock();
    _ = g_table.put(key, {}, common.NEVER_EXPIRES, TIMELESS);
}

/// Drop the memoized answer after a stream read failed against `fleet_id`. A
/// group deleted out-of-band surfaces as a read error, and without clearing the
/// entry this fleet would keep skipping the create and keep failing until the
/// process restarts.
pub fn invalidate(fleet_id: []const u8) void {
    const key = asKey(fleet_id) orelse return;
    g_lock.lock();
    defer g_lock.unlock();
    _ = g_table.remove(key);
}

pub fn resetForTest() void {
    g_lock.lock();
    defer g_lock.unlock();
    g_table.clear();
}

test {
    _ = @import("fleet_group_memo_test.zig");
}
