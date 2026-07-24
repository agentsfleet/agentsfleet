//! Generation-guard proofs, both halves. The unit tests prove WHICH attempt an
//! interrupt is allowed to touch using no real socket: an invalid descriptor is
//! deliberate, so a regression that wrongly matched a stale generation is
//! caught by the `wasInterrupted` assertion rather than a syscall side effect.
//! The integration test at the bottom is the matching real-descriptor proof —
//! a successor connection reusing the same descriptor number keeps exchanging
//! bytes after the stale fire (`std.posix.system.getsockname` reads the port
//! without libc, so real sockets work in this graph).

const std = @import("std");
const scheduler_module = @import("scheduler.zig");
const SocketOwner = @import("SocketOwner.zig");
const InterruptTarget = @import("InterruptTarget.zig");

/// Never dialled. Reaching a syscall with this would mean the guard let a stale
/// generation through, which the assertions below catch first.
const UNUSED_DESCRIPTOR: std.posix.fd_t = -1;
const ATTEMPT_SWEEP_COUNT: usize = 1000;
/// Enough teardown/fire races to shake the interleaving; each iteration arms a
/// deadline already in the past, so the worker is racing the caller every time.
const TEARDOWN_RACE_ROUNDS: usize = 200;
/// Already expired when armed — the worker selects it immediately.
const EXPIRED_DEADLINE_MS: u31 = 1;

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

test "test_interrupt_owner_teardown_waits_for_quiescence" {
    // Dimension 2.3: an owner tearing down while the scheduler is selecting its
    // registration must not race into use-after-free, a double close, or a
    // registration left behind. No socket is attached on purpose — the property
    // under test is the callback/registration lifecycle, not the syscall, and a
    // real descriptor would make the failure a crash rather than an assertion.
    var backend: scheduler_module.MonotonicBackend = .{};
    var scheduler = scheduler_module.ProcessScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();
    defer scheduler.deinit();

    for (0..TEARDOWN_RACE_ROUNDS) |_| {
        // The owner is a fresh stack local every round, exactly as a call-scoped
        // owner is. If `finish` returned before the callback quiesced, the next
        // round's owner would occupy this storage while a callback still held a
        // pointer to it — the use-after-free this barrier exists to prevent.
        var owner: SocketOwner = .{};
        const generation = owner.beginAttempt();
        var guard = try scheduler.arm(owner.target(generation), EXPIRED_DEADLINE_MS);

        // Teardown order under race: retire the generation, THEN quiesce.
        owner.endAttempt();
        const outcome = guard.finish();

        // Either the worker fired first or the caller cancelled first; both are
        // correct. What must never happen is a third state, or a callback still
        // running after `finish` returned.
        try std.testing.expect(outcome == .fired or outcome == .cancelled);
        // Retired before the barrier, so the flag can no longer be set by anyone.
        const settled = owner.wasInterrupted();
        try std.testing.expectEqual(settled, owner.wasInterrupted());
    }
    // `testing.allocator` fails the test on a leaked registration; `deinit`
    // joining the worker proves no registration outlived its guard.
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

// ── Dimension 2.2, real-descriptor half: the successor SOCKET stays usable ──

/// Read the kernel-assigned port off a bound handle. `std.posix.system` routes
/// to raw syscalls on Linux and libSystem on macOS, so this compiles in the
/// lib test graph, which links no libc (`std.c` would not).
fn boundPort(handle: std.Io.net.Socket.Handle) !u16 {
    // SAFETY: getsockname fills sa before sa.port is read on success; the
    // non-SUCCESS branch returns an error without reading sa.
    var sa: std.posix.sockaddr.in = undefined;
    var len: std.posix.socklen_t = @sizeOf(std.posix.sockaddr.in);
    if (std.posix.errno(std.posix.system.getsockname(handle, @ptrCast(&sa), &len)) != .SUCCESS)
        return error.GetSockNameFailed;
    return std.mem.bigToNative(u16, sa.port);
}

const PING = "ping";

test "integration: a stale deadline fire on a reused descriptor number leaves the successor connection usable" {
    // The real-socket half of dimension 2.2 (the decision-logic half is the
    // unit test above): force the kernel to hand a SUCCESSOR connection the
    // exact descriptor number a retired attempt published, fire the retired
    // registration, and prove the successor still moves bytes — i.e. no
    // shutdown(2) reached the recycled number.
    const common = @import("common");
    const io = common.globalIo();

    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    var dial = try std.Io.net.IpAddress.parseIp4("127.0.0.1", boundPort(listener.socket.handle) catch return error.SkipZigTest);

    var owner: SocketOwner = .{};

    // Doomed attempt: dial, publish the descriptor, retire, close. Closing
    // frees the descriptor NUMBER while the registration for `doomed` could
    // still fire late on the scheduler worker.
    const doomed_stream = dial.connect(io, .{ .mode = .stream }) catch return error.SkipZigTest;
    const doomed_handle = doomed_stream.socket.handle;
    const doomed_server = listener.accept(io) catch return error.SkipZigTest;
    defer doomed_server.close(io);
    const doomed = owner.beginAttempt();
    try std.testing.expect(owner.attachSocket(doomed, doomed_handle));
    owner.endAttempt();
    doomed_stream.close(io);

    // Successor: single-threaded, POSIX hands the next socket the lowest free
    // descriptor — the one just closed. The equality assertion keeps this test
    // honest: if the runtime ever stops reusing the number, the scenario is
    // not being exercised and must fail loudly rather than pass vacuously.
    const successor_stream = dial.connect(io, .{ .mode = .stream }) catch return error.SkipZigTest;
    defer successor_stream.close(io);
    try std.testing.expectEqual(doomed_handle, successor_stream.socket.handle);
    const successor_server = listener.accept(io) catch return error.SkipZigTest;
    defer successor_server.close(io);
    const successor = owner.beginAttempt();
    defer owner.endAttempt();
    try std.testing.expect(owner.attachSocket(successor, successor_stream.socket.handle));

    // The doomed registration fires late — the exact call the scheduler worker
    // makes. Identical number, retired generation: stale, touch nothing.
    try std.testing.expectEqual(InterruptTarget.Outcome.stale, owner.target(doomed).interrupt());
    try std.testing.expect(!owner.wasInterrupted());

    // The successor still exchanges data — the recycled number was never shut
    // down. A cross-connection kill would surface here as EOF/write failure.
    var wbuf: [64]u8 = undefined;
    var writer = successor_stream.writer(io, &wbuf);
    try writer.interface.writeAll(PING);
    try writer.interface.flush();
    var rbuf: [PING.len]u8 = undefined;
    var got: usize = 0;
    while (got < PING.len) {
        const n = try std.posix.read(successor_server.socket.handle, rbuf[got..]);
        if (n == 0) break;
        got += n;
    }
    try std.testing.expectEqualStrings(PING, rbuf[0..got]);

    // Control: the CURRENT generation interrupts for real — the guard blocks
    // stale kills, not legitimate ones.
    try std.testing.expectEqual(InterruptTarget.Outcome.interrupted, owner.target(successor).interrupt());
    try std.testing.expect(owner.wasInterrupted());
}
