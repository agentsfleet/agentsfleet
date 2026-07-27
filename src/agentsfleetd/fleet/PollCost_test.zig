//! Unit proofs for the Redis-brownout bailout's counting semantics.
//!
//! The integration proof (`assign_ready_faults_integration_test.zig`) drives the
//! whole candidate loop against a live Redis whose streams cannot be read, and
//! shows the loop stops at the threshold. What it cannot cheaply show is the
//! *negative*: that a healthy candidate between failures resets the run, so a
//! poll scattering failures across many candidates never bails. That property is
//! the difference between "consecutive" and "cumulative", and it is pure
//! arithmetic on this struct — so it is proven here, with no datastore.

const std = @import("std");
const common = @import("common");
const PollCost = @import("PollCost.zig");

const THRESHOLD = common.MAX_CONSECUTIVE_REDIS_FAILURES_PER_POLL;

comptime {
    // The reset test below needs room for a sub-threshold run to exist at all.
    std.debug.assert(THRESHOLD >= 2);
}

test "PollCost: a fresh poll cost is not browned out" {
    const cost = PollCost{};
    try std.testing.expect(!cost.redisBrownedOut());
    try std.testing.expectEqual(@as(u32, 0), cost.redis_failures_in_a_row);
}

test "PollCost: the failure run trips exactly at the threshold, never before it" {
    var cost = PollCost{};
    for (1..THRESHOLD) |_| {
        cost.noteRedisFailure();
        try std.testing.expect(!cost.redisBrownedOut());
    }
    cost.noteRedisFailure();
    try std.testing.expect(cost.redisBrownedOut());
}

test "PollCost: a reachable candidate resets the run, so failures count consecutive not cumulative" {
    var cost = PollCost{};
    // Three sub-threshold bursts, each ended by a candidate whose read reached a
    // verdict. Total failures far exceed the threshold; the longest run does not.
    for (0..3) |_| {
        for (1..THRESHOLD) |_| cost.noteRedisFailure();
        try std.testing.expect(!cost.redisBrownedOut());
        cost.noteRedisReachable();
        try std.testing.expectEqual(@as(u32, 0), cost.redis_failures_in_a_row);
    }
    try std.testing.expect(!cost.redisBrownedOut());
}

test "PollCost: a browned-out cost recovers if a later candidate reaches a verdict" {
    var cost = PollCost{};
    for (0..THRESHOLD) |_| cost.noteRedisFailure();
    try std.testing.expect(cost.redisBrownedOut());
    cost.noteRedisReachable();
    try std.testing.expect(!cost.redisBrownedOut());
}

test "PollCost: round-trip counting accumulates and is independent of the failure run" {
    var cost = PollCost{};
    cost.countDb(1);
    cost.countDb(2);
    cost.noteRedisFailure();
    try std.testing.expectEqual(@as(u64, 3), cost.db_roundtrips);
    cost.noteRedisReachable();
    try std.testing.expectEqual(@as(u64, 3), cost.db_roundtrips);
}
