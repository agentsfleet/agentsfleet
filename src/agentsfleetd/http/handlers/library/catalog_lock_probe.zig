//! The catalog lock probe's round decision, split out from
//! `catalog_etag_integration_test.zig` so it can be proven WITHOUT a live
//! database — and so that file stays under the 350-line cap.
//!
//! The ordering of the three questions below is the whole fix. The probe used
//! to answer only "has a lock waiter appeared within five seconds?", which
//! makes a slow run and a disproved claim indistinguishable: under the
//! coverage lane's kcov instrumentation the patch needs far longer than that
//! to reach its blocking statement, so the lane went red on a correct
//! codebase.

const std = @import("std");

pub const PollVerdict = enum { blocked, keep_waiting, never_blocked, timed_out };

/// One poll round's verdict.
///
/// `lock_waiters` is the count of sessions parked on a `core.fleet_library`
/// lock, `worker_done` is whether the concurrent PATCH has published
/// completion, and `rounds_left` is the backstop budget remaining.
pub fn classifyPollRound(lock_waiters: i64, worker_done: bool, rounds_left: usize) PollVerdict {
    if (lock_waiters > 0) return .blocked;
    // A finished worker that never appeared as a waiter DISPROVES the claim,
    // and does so immediately — waiting out the backstop would only delay a
    // verdict already reached.
    if (worker_done) return .never_blocked;
    // Distinct from `.never_blocked`: nothing was observed either way, so this
    // is a hang or an environment slower than any instrumented run, never
    // evidence about serialization.
    if (rounds_left == 0) return .timed_out;
    return .keep_waiting;
}

test "unit: a patch parked on the lock confirms serialization regardless of worker state" {
    try std.testing.expectEqual(PollVerdict.blocked, classifyPollRound(1, false, 99));
    // A waiter seen on the same round a completion lands still counts: the
    // worker blocked, which is the claim.
    try std.testing.expectEqual(PollVerdict.blocked, classifyPollRound(1, true, 99));
}

test "unit: a completed patch that never took the lock fails immediately" {
    // Dimension 5.1 — the verdict arrives on the round completion is observed,
    // not after the backstop drains.
    try std.testing.expectEqual(PollVerdict.never_blocked, classifyPollRound(0, true, 99));
    try std.testing.expectEqual(PollVerdict.never_blocked, classifyPollRound(0, true, 0));
}

test "unit: an in-flight patch keeps the probe waiting rather than failing" {
    // Dimension 5.2 — this is the round kcov's instrumentation produces for
    // thousands of iterations; it must not be an assertion failure.
    try std.testing.expectEqual(PollVerdict.keep_waiting, classifyPollRound(0, false, 1));
}

test "unit: exhausting the backstop is reported apart from never having blocked" {
    // Dimension 5.3 — a hang and a disproved claim must never be read for each
    // other, which is exactly what the old single error conflated.
    try std.testing.expectEqual(PollVerdict.timed_out, classifyPollRound(0, false, 0));
}
