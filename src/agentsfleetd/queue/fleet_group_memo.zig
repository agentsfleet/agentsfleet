//! Per-process memo of which fleets already have their `fleet_lease` consumer
//! group created.
//!
//! `XGROUP CREATE … MKSTREAM` is idempotent, so the unmemoized form was correct
//! but paid one Redis round-trip per candidate per lease poll, using the
//! `BUSYGROUP` error reply as its steady state. The group is durable: once
//! created it stays created, so the answer is memoizable for the process
//! lifetime.
//!
//! **Deliberately the simplest structure that works.** A direct-mapped array of
//! hashes — no probing, no slot-claim protocol, no stored ids. Every way this can
//! be wrong costs exactly one redundant Redis command:
//!
//!   - Two fleets hash to the same index: one evicts the other, and the evicted
//!     one issues a real `XGROUP CREATE` on its next poll.
//!   - Two threads record concurrently: they write the same value, or one
//!     overwrites the other's index. Either way the next reader is correct or
//!     pays one command.
//!   - A 64-bit hash collision between two distinct fleets: the second reads as
//!     ensured without a create. Its following stream read fails, `invalidate`
//!     clears the entry, and the next poll creates the group for real.
//!
//! That last case is the only one that could skip a genuinely needed create, and
//! it self-heals through the same path an out-of-band group deletion takes. None
//! of these is a correctness question, which is why this file carries none of the
//! compare-and-swap claim discipline that `observability/metrics_runner.zig` needs
//! — there, a lost slot loses data; here it costs a round-trip.

const std = @import("std");

/// Distinct fleets tracked per process. 8 bytes each, so the whole table is 32
/// KiB and needs no allocator.
pub const MAX_SLOTS: usize = 4096;

/// Reserved to mean "empty", so a real hash never collides with the empty state.
const EMPTY: u64 = 0;

// safe because: each slot is an independent hint whose readers tolerate a stale
// answer in both directions — a stale "ensured" costs one failed read plus an
// invalidate, a stale "absent" costs one redundant XGROUP CREATE. No other memory
// is published through these atomics, so `.monotonic` is sufficient.
var g_ensured: [MAX_SLOTS]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(EMPTY)} ** MAX_SLOTS;

/// Never returns `EMPTY`, so the sentinel stays unambiguous.
fn fleetHash(fleet_id: []const u8) u64 {
    var h = std.hash.Wyhash.init(0);
    h.update(fleet_id);
    const value = h.final();
    return if (value == EMPTY) 1 else value;
}

fn slot(h: u64) *std.atomic.Value(u64) {
    return &g_ensured[h % MAX_SLOTS];
}

/// True when this process has already created `fleet_id`'s consumer group and
/// nothing has invalidated that. A false answer costs one Redis command.
pub fn isEnsured(fleet_id: []const u8) bool {
    const h = fleetHash(fleet_id);
    return slot(h).load(.monotonic) == h; // safe because: see module note above
}

/// Record that `fleet_id`'s group exists. Call only after a create that returned
/// OK or reported BUSYGROUP — both prove the group is present.
pub fn recordEnsured(fleet_id: []const u8) void {
    const h = fleetHash(fleet_id);
    slot(h).store(h, .monotonic); // safe because: see module note above
}

/// Drop the memoized answer after a stream read failed against `fleet_id`. A
/// group deleted out-of-band surfaces as a read error, and without clearing the
/// entry this fleet would keep skipping the create and keep failing until the
/// process restarts.
///
/// Compare-and-clear rather than an unconditional store, so a fleet that has
/// since evicted this one out of the index is left alone.
pub fn invalidate(fleet_id: []const u8) void {
    const h = fleetHash(fleet_id);
    _ = slot(h).cmpxchgStrong(h, EMPTY, .monotonic, .monotonic); // safe because: see module note above
}

pub fn resetForTest() void {
    for (&g_ensured) |*entry| entry.store(EMPTY, .monotonic); // safe because: single-threaded test reset
}

test {
    _ = @import("fleet_group_memo_test.zig");
}
