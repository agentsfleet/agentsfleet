//! Attempt lifecycle for the runner control-plane client: fail-closed arming,
//! generation-scoped interruption, and an idempotent release barrier.

const std = @import("std");
const testing = std.testing;
const common = @import("common");
const call_deadline = @import("call_deadline");
const deadline = @import("control_plane_deadline.zig");

/// Well under any suite timeout, well over the scheduler's wake latency.
const FIRE_MS: u31 = 100;
/// A deadline no test waits for — proves a path returns without one firing.
const NEVER_MS: u31 = 60_000;
const QUIESCE_BOUND_MS: i64 = 5_000;
/// Poll pacing while waiting on the deadline worker. A bare spin never yields,
/// so under a serialized thread scheduler (valgrind runs one thread at a time)
/// the waiter starves the very worker it waits for and the fire lands late.
const QUIESCE_POLL_NS: u64 = std.time.ns_per_ms;

/// A started process scheduler, as the runner root owns one.
const TestScheduler = struct {
    backend: call_deadline.MonotonicBackend = .{},
    sched: ?deadline.Scheduler = null,

    fn start(self: *TestScheduler) !void {
        self.sched = deadline.Scheduler.init(testing.allocator, &self.backend);
        try self.sched.?.start();
    }

    fn deinit(self: *TestScheduler) void {
        if (self.sched) |*s| s.deinit();
    }
};

/// A loopback socket to attach — a real descriptor, so `shutdown(2)` on a fire
/// hits a live handle rather than a number the kernel never issued.
const LoopbackSocket = struct {
    listener: std.Io.net.Server,
    io: std.Io,

    fn open(io: std.Io) !LoopbackSocket {
        var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
        return .{ .listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest, .io = io };
    }

    fn handle(self: *LoopbackSocket) std.posix.fd_t {
        return self.listener.socket.handle;
    }

    fn deinit(self: *LoopbackSocket) void {
        self.listener.deinit(self.io);
    }
};

test "a pin failure refuses the verb instead of running it unarmed" {
    var runner: TestScheduler = .{};
    try runner.start();
    defer runner.deinit();

    var attempt: deadline.Attempt = .{};
    attempt.begin();
    defer attempt.release();

    // No pooled handle: the ONLY correct answer is a refusal. Falling through
    // here is the unbounded call deadlines exist to prevent.
    try testing.expectEqual(deadline.ArmOutcome.pin_failed, attempt.armPinned(&runner.sched.?, null, NEVER_MS));
}

test "an unstartable scheduler refuses the verb fail-closed" {
    const io = common.globalIo();
    var sock = try LoopbackSocket.open(io);
    defer sock.deinit();

    // Never started — the fail-closed path a stopping process takes.
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = deadline.Scheduler.init(testing.allocator, &backend);
    defer sched.deinit();

    var attempt: deadline.Attempt = .{};
    attempt.begin();
    defer attempt.release();

    try testing.expectEqual(
        deadline.ArmOutcome.scheduler_unavailable,
        attempt.armPinned(&sched, sock.handle(), NEVER_MS),
    );
}

test "a fired deadline marks its own attempt interrupted" {
    const io = common.globalIo();
    var sock = try LoopbackSocket.open(io);
    defer sock.deinit();

    var runner: TestScheduler = .{};
    try runner.start();
    defer runner.deinit();

    var attempt: deadline.Attempt = .{};
    attempt.begin();
    defer attempt.release();
    try testing.expectEqual(
        deadline.ArmOutcome.armed,
        attempt.armPinned(&runner.sched.?, sock.handle(), FIRE_MS),
    );

    const t0 = common.clock.nowMillis();
    while (!attempt.wasInterrupted() and common.clock.nowMillis() - t0 < QUIESCE_BOUND_MS) {
        common.sleepNanos(QUIESCE_POLL_NS);
    }
    try testing.expect(attempt.wasInterrupted());
}

test "release retires the generation so a late fire cannot reach a successor" {
    const io = common.globalIo();
    var sock = try LoopbackSocket.open(io);
    defer sock.deinit();

    var runner: TestScheduler = .{};
    try runner.start();
    defer runner.deinit();

    // Attempt one arms and is released BEFORE its deadline could fire.
    var attempt: deadline.Attempt = .{};
    attempt.begin();
    try testing.expectEqual(
        deadline.ArmOutcome.armed,
        attempt.armPinned(&runner.sched.?, sock.handle(), NEVER_MS),
    );
    attempt.release();

    // The successor reuses the same control block and the same descriptor —
    // the exact shape a descriptor-number deadline could not tell apart. The
    // retired registration names the old generation, so it is inert.
    attempt.begin();
    try testing.expectEqual(
        deadline.ArmOutcome.armed,
        attempt.armPinned(&runner.sched.?, sock.handle(), NEVER_MS),
    );
    try testing.expect(!attempt.wasInterrupted());
    attempt.release();

    // Idempotent: a second release finds no guard and does nothing.
    attempt.release();
    try testing.expect(!attempt.wasInterrupted());
}
