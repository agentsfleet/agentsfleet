//! Unit tests for the consumer-group memo.
//!
//! The memo needs no datastore, so every property is provable here: what it
//! answers before and after a recorded creation, that a lookup miss does not
//! consume a slot, that overflow degrades to "not ensured" rather than to a wrong
//! "ensured", that invalidation restores the real path, and that two threads
//! racing for the same fresh slot both end up with a correct answer.
//!
//! Every test resets the process-global table first — these are globals shared
//! across the whole test binary, and Zig runs tests sequentially in one process.

const std = @import("std");
const memo = @import("fleet_group_memo.zig");

const testing = std.testing;

const FLEET_A = "0192f3c1-7a4b-7def-8123-4567890abcde";
const FLEET_B = "0192f3c1-7a4b-7def-8123-4567890abcdf";

/// Interleavings each side of the flip/read race performs. High enough that the
/// two threads genuinely overlap on a loaded machine, low enough to stay a unit
/// test — the property is "no torn read", which one overlap already exercises.
const RACE_ITERATIONS: usize = 1000;

test "a fleet is not ensured until a creation is recorded" {
    memo.resetForTest();
    try testing.expect(!memo.isEnsured(FLEET_A));
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "recording one fleet does not mark a different fleet ensured" {
    // The two ids above differ only in their final character — a slot match that
    // compared hashes alone, or compared a too-short prefix, would conflate them
    // and skip a genuinely needed XGROUP CREATE.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
    try testing.expect(!memo.isEnsured(FLEET_B));
}

test "repeated recording of one fleet stays a single answer" {
    memo.resetForTest();
    for (0..10) |_| memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "a lookup miss consumes no slot" {
    // `isEnsured` must never claim: capacity is finite, and a table filled by
    // fleets whose groups were never confirmed would push fleets that DO have
    // groups into overflow, costing them a Redis command on every poll forever.
    memo.resetForTest();
    var probe_buf: [40]u8 = undefined;
    for (0..memo.MAX_SLOTS * 2) |i| {
        const id = try std.fmt.bufPrint(&probe_buf, "absent-{d}", .{i});
        try testing.expect(!memo.isEnsured(id));
    }
    // The table is still completely free, so a real recording lands and reads back.
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "invalidation clears the answer but keeps the fleet resident" {
    // This is the out-of-band group deletion path: the read failed, so the memo
    // must stop claiming the group exists. Residency is retained so the entry is
    // re-verified rather than the key being evicted and re-competing for a slot.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));

    memo.invalidate(FLEET_A);
    try testing.expect(!memo.isEnsured(FLEET_A));

    // Re-creating after the invalidation works — the fleet is not permanently
    // poisoned, which would fail that fleet until process restart.
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "invalidating a fleet that was never recorded is a no-op" {
    memo.resetForTest();
    memo.invalidate(FLEET_A);
    try testing.expect(!memo.isEnsured(FLEET_A));
}

test "a fleet past capacity reads as not ensured rather than as ensured" {
    // Overflow must fail toward doing the real work. Answering "ensured" for an
    // unrecorded fleet would skip a create the fleet may genuinely need; the
    // stream read would then fail and that fleet would never lease.
    memo.resetForTest();
    var id_buf: [40]u8 = undefined;
    for (0..memo.MAX_SLOTS) |i| {
        const id = try std.fmt.bufPrint(&id_buf, "fill-{d}", .{i});
        memo.recordEnsured(id);
    }
    const overflow_id = "overflow-fleet-id";
    memo.recordEnsured(overflow_id);
    try testing.expect(!memo.isEnsured(overflow_id));

    // A fleet that did get a slot still answers correctly, so overflow degrades
    // only the fleets past the bound.
    const first = try std.fmt.bufPrint(&id_buf, "fill-{d}", .{0});
    try testing.expect(memo.isEnsured(first));
}

const Racer = struct {
    fn record(id: []const u8) void {
        memo.recordEnsured(id);
    }
};

test "two threads claiming the same fresh slot both leave it ensured" {
    // The claim window is a load→cmpxchg pair, so exactly one thread wins and the
    // loser must re-inspect the SAME index rather than probing past it — the
    // winner may have claimed that slot for the loser's own key. The barrier
    // parks both threads inside that window so the race is exercised rather than
    // hoped for.
    memo.resetForTest();
    memo.setClaimBarrierForTest(2);

    const a = try std.Thread.spawn(.{}, Racer.record, .{FLEET_A});
    const b = try std.Thread.spawn(.{}, Racer.record, .{FLEET_A});
    a.join();
    b.join();

    memo.setClaimBarrierForTest(0);
    try testing.expect(memo.isEnsured(FLEET_A));

    // And the race consumed one key, not two: a distinct fleet still resolves
    // independently afterwards.
    memo.recordEnsured(FLEET_B);
    try testing.expect(memo.isEnsured(FLEET_B));
}

test "concurrent readers and an invalidator never observe a torn entry" {
    // `ensured` is read with .acquire against a .release store, so a reader sees
    // either the old or the new verdict — never a partially published slot whose
    // id bytes belong to one fleet and whose verdict belongs to another.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);

    const Flipper = struct {
        fn run(id: []const u8) void {
            for (0..RACE_ITERATIONS) |i| {
                if (i % 2 == 0) memo.invalidate(id) else memo.recordEnsured(id);
            }
        }
    };
    const Reader = struct {
        fn run(id: []const u8) void {
            // Both answers are legitimate mid-flip; what must not happen is a
            // match against the wrong key, which would trip the id assertion in
            // slotMatches and surface as a wrong-fleet answer.
            for (0..RACE_ITERATIONS) |_| std.mem.doNotOptimizeAway(memo.isEnsured(id));
        }
    };

    const flipper = try std.Thread.spawn(.{}, Flipper.run, .{FLEET_A});
    const reader = try std.Thread.spawn(.{}, Reader.run, .{FLEET_A});
    flipper.join();
    reader.join();

    // A distinct fleet was never touched by either thread and must still be absent.
    try testing.expect(!memo.isEnsured(FLEET_B));
}
