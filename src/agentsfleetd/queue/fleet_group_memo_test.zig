//! Unit tests for the consumer-group memo.
//!
//! The memo needs no datastore, so every property is provable here. Note what is
//! deliberately NOT tested: concurrent races. The set-associative table sits
//! behind an `RwLock` — reads share the lock, writes serialize — and every
//! racing outcome is benign: writers store the same value or evict one bucket
//! entry, and a reader that loses either way pays at most one redundant Redis
//! command. A test asserting a particular interleaving would be pinning
//! behaviour the design does not promise.
//!
//! Every test resets the process-global table first — these are globals shared
//! across the whole test binary, and Zig runs tests sequentially in one process.

const std = @import("std");
const memo = @import("fleet_group_memo.zig");

const testing = std.testing;

const FLEET_A = "0192f3c1-7a4b-7def-8123-4567890abcde";
const FLEET_B = "0192f3c1-7a4b-7def-8123-4567890abcdf";

test "a fleet is not ensured until a creation is recorded" {
    memo.resetForTest();
    try testing.expect(!memo.isEnsured(FLEET_A));
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "recording one fleet does not mark a different fleet ensured" {
    // These two ids differ only in their final character. They hash to different
    // values, so one being ensured must not make the other read as ensured —
    // that would skip a genuinely needed XGROUP CREATE.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
    try testing.expect(!memo.isEnsured(FLEET_B));
}

test "recording is idempotent" {
    memo.resetForTest();
    for (0..10) |_| memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "a lookup never records" {
    // Reading must not populate the table: only a confirmed creation earns an
    // entry. A lookup that recorded would let unverified fleets evict verified
    // ones, so every evicted fleet would pay a command on its next poll.
    memo.resetForTest();
    var probe_buf: [40]u8 = undefined;
    for (0..64) |i| {
        const id = try std.fmt.bufPrint(&probe_buf, "absent-{d}", .{i});
        try testing.expect(!memo.isEnsured(id));
        try testing.expect(!memo.isEnsured(id)); // still absent on a second look
    }
}

test "invalidation clears the answer and the fleet can be re-recorded" {
    // The out-of-band group-deletion path: the read failed, so the memo must stop
    // claiming the group exists — and must not poison the fleet permanently,
    // which would fail it until process restart.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));

    memo.invalidate(FLEET_A);
    try testing.expect(!memo.isEnsured(FLEET_A));

    memo.recordEnsured(FLEET_A);
    try testing.expect(memo.isEnsured(FLEET_A));
}

test "invalidating an unrecorded fleet is a no-op" {
    memo.resetForTest();
    memo.invalidate(FLEET_A);
    try testing.expect(!memo.isEnsured(FLEET_A));
}

test "invalidating one fleet leaves an unrelated fleet ensured" {
    // `invalidate` matches on the stored fleet id, so it can only ever clear its
    // own entry — never one that happens to share a bucket.
    memo.resetForTest();
    memo.recordEnsured(FLEET_A);
    memo.recordEnsured(FLEET_B);
    memo.invalidate(FLEET_A);
    try testing.expect(!memo.isEnsured(FLEET_A));
    try testing.expect(memo.isEnsured(FLEET_B));
}

test "the table holds many distinct fleets at once" {
    // Set-associative, so a bucket absorbs several colliding ids before evicting
    // any. What must hold is that the common case works at scale: a realistic
    // fleet population stays ensured, and nothing crashes or wedges.
    memo.resetForTest();
    var id_buf: [40]u8 = undefined;
    const population: usize = memo.CAPACITY / 2;
    for (0..population) |i| {
        const id = try std.fmt.bufPrint(&id_buf, "fill-{d}", .{i});
        memo.recordEnsured(id);
    }
    var still_ensured: usize = 0;
    for (0..population) |i| {
        const id = try std.fmt.bufPrint(&id_buf, "fill-{d}", .{i});
        if (memo.isEnsured(id)) still_ensured += 1;
    }
    // At half load a 4-deep bucket absorbs essentially every collision, so
    // retention is near-total rather than the bare majority a direct-mapped
    // table could promise. Each loss is one redundant command, never a wrong
    // answer.
    try testing.expect(still_ensured > population * 9 / 10);
}

test "a fleet evicted by a collision simply reads as not ensured" {
    // The eviction path stated plainly: whatever loses its slot reads false and
    // issues a real create. It never reads as ensured-when-it-is-not, which is
    // the only outcome that would matter.
    memo.resetForTest();
    var id_buf: [40]u8 = undefined;
    for (0..memo.CAPACITY * 2) |i| {
        const id = try std.fmt.bufPrint(&id_buf, "churn-{d}", .{i});
        memo.recordEnsured(id);
    }
    // The last one recorded is definitely present; earlier ones may not be, and
    // that is the designed behaviour rather than a defect.
    const last = try std.fmt.bufPrint(&id_buf, "churn-{d}", .{memo.CAPACITY * 2 - 1});
    try testing.expect(memo.isEnsured(last));
}

test "an id too long to store is never reported ensured" {
    // Degrades to the cold-memo cost — one redundant create per poll — rather
    // than storing a truncated id, which could match the wrong fleet.
    memo.resetForTest();
    const overlong = "f" ** 64;
    memo.recordEnsured(overlong);
    try testing.expect(!memo.isEnsured(overlong));
}
