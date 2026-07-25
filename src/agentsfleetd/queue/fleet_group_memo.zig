//! Per-process memo of which fleets already have their `fleet_lease` consumer
//! group created.
//!
//! `XGROUP CREATE … MKSTREAM` is idempotent, so the unmemoized form was correct
//! but paid one Redis round-trip per candidate per lease poll forever, using the
//! `BUSYGROUP` error reply as its steady state. The group is durable: once
//! created it stays created, so the answer is memoizable for the process
//! lifetime.
//!
//! Fixed-capacity and allocator-free, mirroring the slot table in
//! `observability/metrics_runner.zig` — same constraints (a bounded key space, no
//! allocator on a hot path, a defined overflow behaviour), so the same shape.
//! Overflow is not a correctness question: a fleet that finds no slot simply
//! takes the real path every poll, which is exactly today's behaviour.
//!
//! Truncation is likewise safe by construction. An id longer than `ID_LEN` is
//! compared on its prefix, so two such ids could collide and one fleet could
//! wrongly read as ensured. That skips a create for a group that may not exist,
//! the following stream read fails, `invalidate` drops the entry, and the next
//! poll creates it for real — one wasted poll, self-healing. Canonical fleet ids
//! are 36-char UUIDs and fit outright.

const std = @import("std");
const builtin = @import("builtin");

/// Distinct fleets tracked per process. Beyond this, `isEnsured` answers false
/// and the caller issues the real command.
pub const MAX_SLOTS: usize = 4096;
/// Bytes of fleet id held per slot. A canonical UUID is 36; the slack absorbs
/// the non-UUID ids test fixtures use.
const ID_LEN: usize = 48;
/// How long a reader waits for another thread's in-flight `initSlot` before
/// giving up. Bounded so a descheduled initializer can never stall a lease poll;
/// answering "not ensured" costs one redundant Redis command, which is strictly
/// better than parking a request thread.
const READY_SPIN_CAP: u32 = 4096;

const SLOT_FREE: u8 = 0;
const SLOT_TAKEN: u8 = 1;

// safe because: every atomic below is either an independent flag whose readers
// tolerate a stale answer (both stale directions cost at most one redundant
// Redis command, never a correctness change), or the `occupied`/`ready` pair
// whose release-store in initSlot publishes the id bytes to acquire-loading
// readers. No other memory is published through these atomics.

const Slot = struct {
    occupied: std.atomic.Value(u8) = std.atomic.Value(u8).init(SLOT_FREE),
    ready: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    /// 1 once the group is known created; cleared by `invalidate` while the slot
    /// stays resident, so the key keeps its place in the table.
    ensured: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    fleet_id: [ID_LEN]u8 = [_]u8{0} ** ID_LEN,
    fleet_id_len: u8 = 0,
    hash: u64 = 0,
};

var g_slots: [MAX_SLOTS]Slot = [_]Slot{.{}} ** MAX_SLOTS;
var g_slot_count = std.atomic.Value(u32).init(0);

fn fleetHash(fleet_id: []const u8) u64 {
    var h = std.hash.Wyhash.init(0);
    h.update(fleet_id);
    return h.final();
}

fn slotMatches(slot: *const Slot, h: u64, fleet_id: []const u8) bool {
    if (slot.hash != h) return false;
    const cmp = @min(fleet_id.len, ID_LEN);
    if (slot.fleet_id_len != cmp) return false;
    return std.mem.eql(u8, slot.fleet_id[0..slot.fleet_id_len], fleet_id[0..cmp]);
}

fn initSlot(slot: *Slot, h: u64, fleet_id: []const u8) void {
    const len: u8 = @intCast(@min(fleet_id.len, ID_LEN));
    @memcpy(slot.fleet_id[0..len], fleet_id[0..len]);
    slot.fleet_id_len = len;
    slot.hash = h;
    slot.ready.store(1, .release); // safe because: publishes the id writes above to readers loading ready with .acquire
}

/// Spin until a claimed slot's initializer publishes it. `false` ⇒ the slot
/// outlasted the bounded spin and the caller treats the key as absent.
fn awaitReady(slot: *const Slot) bool {
    var spins: u32 = 0;
    while (slot.ready.load(.acquire) != 1) { // safe because: pairs with the .release store in initSlot
        if (spins >= READY_SPIN_CAP) return false;
        std.atomic.spinLoopHint();
        spins += 1;
    }
    return true;
}

/// Read-only probe. Deliberately never claims a slot: only a confirmed creation
/// earns residency, so a lookup miss must not consume capacity that a fleet with
/// a real group needs. The first unoccupied index proves absence, because a slot
/// is never un-occupied once claimed.
fn findSlot(fleet_id: []const u8) ?*Slot {
    const h = fleetHash(fleet_id);
    const start = h % MAX_SLOTS;
    var i: usize = 0;
    while (i < MAX_SLOTS) : (i += 1) {
        const slot = &g_slots[(start + i) % MAX_SLOTS];
        if (slot.occupied.load(.acquire) == SLOT_FREE) return null; // safe because: pairs with the cmpxchg release on claim
        if (!awaitReady(slot)) return null;
        if (slotMatches(slot, h, fleet_id)) return slot;
    }
    return null;
}

/// Linear-probe to the fleet's slot, claiming a fresh one on first sight. Null
/// when every slot is occupied (capacity overflow) or a slot stuck mid-init
/// outlasts the bounded spin. Never advances past a slot without ruling it out:
/// a lost claim re-examines the same index, since the winner may have claimed it
/// for OUR key.
fn resolveSlot(fleet_id: []const u8) ?*Slot {
    const h = fleetHash(fleet_id);
    const start = h % MAX_SLOTS;
    var i: usize = 0;
    while (i < MAX_SLOTS) : (i += 1) {
        const slot = &g_slots[(start + i) % MAX_SLOTS];
        if (slot.occupied.load(.acquire) == SLOT_FREE) { // safe because: pairs with the cmpxchg release on claim
            claimBarrierForTest();
            if (slot.occupied.cmpxchgStrong(SLOT_FREE, SLOT_TAKEN, .acq_rel, .acquire) == null) {
                initSlot(slot, h, fleet_id);
                _ = g_slot_count.fetchAdd(1, .monotonic); // safe because: independent counter, no ordering dependency
                return slot;
            }
            // Lost the claim by a hair — fall through and inspect THIS slot.
        }
        if (!awaitReady(slot)) return null;
        if (slotMatches(slot, h, fleet_id)) return slot;
    }
    return null;
}

/// True when this process has already created `fleet_id`'s consumer group and
/// nothing has invalidated that. A false answer only costs one Redis command.
pub fn isEnsured(fleet_id: []const u8) bool {
    const slot = findSlot(fleet_id) orelse return false;
    return slot.ensured.load(.acquire) == 1; // safe because: pairs with the release store in recordEnsured
}

/// Record that `fleet_id`'s group exists. Call only after a create that either
/// returned OK or reported BUSYGROUP — both prove the group is present.
pub fn recordEnsured(fleet_id: []const u8) void {
    const slot = resolveSlot(fleet_id) orelse return;
    slot.ensured.store(1, .release); // safe because: pairs with the acquire load in isEnsured
}

/// Drop the memoized answer for `fleet_id` after a stream read failed against
/// it. The slot stays resident — only the verdict is cleared — so a group
/// deleted out-of-band is recreated on the next poll rather than failing that
/// fleet until the process restarts.
pub fn invalidate(fleet_id: []const u8) void {
    const slot = findSlot(fleet_id) orelse return;
    slot.ensured.store(0, .release); // safe because: pairs with the acquire load in isEnsured
}

/// Test-only arrival barrier inside the load→cmpxchg claim window, so a test can
/// drive two threads into the same race deterministically (rationale in
/// `fleet_group_memo_test.zig`).
var g_claim_barrier_target: u64 = 0; // written single-threaded, before contenders spawn
var g_claim_barrier_arrivals = std.atomic.Value(u64).init(0);

pub fn setClaimBarrierForTest(target: u64) void {
    g_claim_barrier_target = target;
    g_claim_barrier_arrivals.store(0, .release); // safe because: no contender is running when the barrier is (re)armed
}

inline fn claimBarrierForTest() void {
    if (!builtin.is_test) return;
    const target = g_claim_barrier_target;
    if (target == 0) return;
    // The counter only grows past `target`, so a late arrival passes through.
    _ = g_claim_barrier_arrivals.fetchAdd(1, .acq_rel);
    while (g_claim_barrier_arrivals.load(.acquire) < target) std.atomic.spinLoopHint(); // safe because: pairs with the fetchAdd above across contenders
}

pub fn resetForTest() void {
    g_slots = [_]Slot{.{}} ** MAX_SLOTS;
    g_slot_count.store(0, .release); // safe because: single-threaded test reset
    g_claim_barrier_target = 0; // a barrier left armed would deadlock the next single-threaded claim
    g_claim_barrier_arrivals.store(0, .release);
}

test {
    _ = @import("fleet_group_memo_test.zig");
}
