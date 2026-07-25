//! `call_deadline.reached` — the fail-fast half of the deadline story.
//!
//! Single-threaded Io throughout: these tests read a clock and nothing else, and
//! a worker pool would only add the very `std.Io.Threaded` machinery this helper
//! exists to keep callers away from.

const std = @import("std");
const call_deadline = @import("call_deadline.zig");

fn bootNow(io: std.Io) i96 {
    return std.Io.Clock.boot.now(io).toNanoseconds();
}

test "a spent budget refuses the call before it starts" {
    var threaded: std.Io.Threaded = .init_single_threaded;
    const io = threaded.io();

    try std.testing.expect(call_deadline.reached(io, bootNow(io) - std.time.ns_per_s));
}

test "a budget with time left lets the call start" {
    var threaded: std.Io.Threaded = .init_single_threaded;
    const io = threaded.io();

    try std.testing.expect(!call_deadline.reached(io, bootNow(io) + std.time.ns_per_s));
}

test "the deadline instant itself counts as spent" {
    var threaded: std.Io.Threaded = .init_single_threaded;
    const io = threaded.io();

    // Sampled, then passed straight back: the clock is at or past it either way,
    // which pins the `>=` and rules out a `>` that would let one call through on
    // the exact nanosecond.
    try std.testing.expect(call_deadline.reached(io, bootNow(io)));
}

test "a deadline of zero is always spent rather than treated as absent" {
    var threaded: std.Io.Threaded = .init_single_threaded;
    const io = threaded.io();

    // Callers pass `?SetupBudget`; absence is the null, never a zero deadline.
    // A zero reaching here means a caller lost its budget, and refusing is the
    // safe reading.
    try std.testing.expect(call_deadline.reached(io, 0));
}
