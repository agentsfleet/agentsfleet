//! Owner-mediated interruption handle handed to the deadline scheduler.
//!
//! The scheduler never learns a socket descriptor. It holds this handle and, at
//! the deadline, asks the OWNER to interrupt one specific connection
//! generation. The owner compares that generation against whatever it currently
//! holds, under its own lock, and only then touches a socket.
//!
//! That indirection is the whole point. A descriptor number is not an identity:
//! a connection pool recycles it, so a deadline armed for a finished call could
//! otherwise shut down a successor connection that merely inherited the number.
//! A generation is non-repeating, so a late fire against a replaced connection
//! is provably a no-op instead of a silent cross-connection kill.

const InterruptTarget = @This();

/// What the owner did when the deadline fired.
pub const Outcome = enum {
    /// The owner still held this exact generation and interrupted it.
    interrupted,
    /// The generation had already been replaced or completed. Nothing was
    /// touched — this is the expected result of a late fire, not an error.
    stale,
};

/// Borrowed control block. The owner keeps it address-stable for as long as a
/// registration naming it can still fire, which `Guard.finish` guarantees.
ctx: *anyopaque,
/// Owner-supplied validation-and-interrupt step. See `interrupt`.
interruptFn: *const fn (ctx: *anyopaque, expected_generation: u64) Outcome,
/// The connection generation this registration was armed against.
generation: u64,

/// Runs on the sole scheduler worker. Implementations must be bounded,
/// nonblocking, non-reentrant leaves: they may take the owner's lock and issue
/// one shutdown syscall, but must never call back into the scheduler (`arm`,
/// `finish`, `stop`), or the worker deadlocks against its own barrier.
pub fn interrupt(self: InterruptTarget) Outcome {
    return self.interruptFn(self.ctx, self.generation);
}

/// How long a conforming `interrupt` may take. A lock plus one `shutdown(2)`
/// completes in microseconds, so this sits orders of magnitude above any
/// legitimate run — a breach means the bounded-leaf rule above was broken, not
/// that the machine was briefly busy.
pub const CALLBACK_BUDGET_NS: i96 = 250 * std.time.ns_per_ms;

/// The scheduler reports a breach instead of asserting: every deadline in the
/// process shares one worker, so a blocking callback stalls them all, and the
/// operator needs the culprit named rather than the stall left as an
/// unexplained latency mystery. Deliberately not fatal — turning a slow syscall
/// into a crash would be worse than the stall it reports.
pub fn overranBudget(elapsed_ns: i96) bool {
    return elapsed_ns >= CALLBACK_BUDGET_NS;
}

/// Saturating milliseconds for the breach log — a nonsense clock reading must
/// not panic the scheduler worker on an `@intCast` while reporting a stall.
pub fn elapsedMillis(elapsed_ns: i96) i64 {
    return std.math.cast(i64, @divTrunc(elapsed_ns, std.time.ns_per_ms)) orelse std.math.maxInt(i64);
}

test {
    _ = @import("InterruptTarget_test.zig");
}

const std = @import("std");
