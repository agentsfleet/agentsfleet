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

test {
    _ = @import("InterruptTarget_test.zig");
}
