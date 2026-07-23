//! Generation-guard decision logic. These prove WHICH attempt an interrupt is
//! allowed to touch, using no real socket: an invalid descriptor is deliberate,
//! so a regression that wrongly matched a stale generation would be caught by
//! the `wasInterrupted` assertion rather than by a syscall side effect. The
//! matching real-descriptor proof — that a successor connection reusing the
//! same descriptor number stays usable — lives in the daemon's integration
//! suite, which has actual sockets.

const std = @import("std");
const SocketOwner = @import("SocketOwner.zig");
const InterruptTarget = @import("InterruptTarget.zig");

/// Never dialled. Reaching a syscall with this would mean the guard let a stale
/// generation through, which the assertions below catch first.
const UNUSED_DESCRIPTOR: std.posix.fd_t = -1;
const ATTEMPT_SWEEP_COUNT: usize = 1000;

test "test_interrupt_targets_exact_connection_generation" {
    var owner: SocketOwner = .{};

    const first = owner.beginAttempt();
    // The live generation is interruptible, and the owner records it so a setup
    // loop can abort between stages.
    try std.testing.expectEqual(InterruptTarget.Outcome.interrupted, owner.target(first).interrupt());
    try std.testing.expect(owner.wasInterrupted());

    // Retiring and starting a fresh attempt clears the record ...
    owner.endAttempt();
    const second = owner.beginAttempt();
    try std.testing.expect(!owner.wasInterrupted());
    try std.testing.expect(second != first);

    // ... and the previous generation's registration is now inert. This is the
    // late-fire case: the deadline was armed for work that already finished.
    try std.testing.expectEqual(InterruptTarget.Outcome.stale, owner.target(first).interrupt());
    try std.testing.expect(!owner.wasInterrupted());

    // The current generation still interrupts normally.
    try std.testing.expectEqual(InterruptTarget.Outcome.interrupted, owner.target(second).interrupt());
    try std.testing.expect(owner.wasInterrupted());
}

test "test_stale_deadline_cannot_interrupt_reused_descriptor" {
    var owner: SocketOwner = .{};

    // An attempt publishes a descriptor, then completes.
    const doomed = owner.beginAttempt();
    try std.testing.expect(owner.attachSocket(doomed, UNUSED_DESCRIPTOR));
    owner.endAttempt();

    // A successor attempt inherits the SAME descriptor number — exactly what a
    // connection pool does when it recycles a closed connection's slot.
    const successor = owner.beginAttempt();
    try std.testing.expect(owner.attachSocket(successor, UNUSED_DESCRIPTOR));

    // The doomed attempt's deadline now fires. Identical descriptor, different
    // generation: the successor must be left completely alone.
    try std.testing.expectEqual(InterruptTarget.Outcome.stale, owner.target(doomed).interrupt());
    try std.testing.expect(!owner.wasInterrupted());
}

test "attachSocket refuses a generation the owner has already retired" {
    var owner: SocketOwner = .{};
    const retired = owner.beginAttempt();
    owner.endAttempt();
    // The caller still holds the socket and must close it itself.
    try std.testing.expect(!owner.attachSocket(retired, UNUSED_DESCRIPTOR));
}

test "generations never repeat across many attempts" {
    var owner: SocketOwner = .{};
    var previous: u64 = 0;
    for (0..ATTEMPT_SWEEP_COUNT) |_| {
        const current = owner.beginAttempt();
        try std.testing.expect(current > previous);
        previous = current;
        owner.endAttempt();
    }
    // Generation 0 means "no attempt yet" and must never match a registration.
    try std.testing.expectEqual(InterruptTarget.Outcome.stale, owner.target(0).interrupt());
}
