const std = @import("std");
const common = @import("common");
const scheduler_module = @import("scheduler.zig");
const InterruptTarget = @import("InterruptTarget.zig");

const FakeBackend = struct {
    monotonic_ns: std.atomic.Value(i64) = std.atomic.Value(i64).init(0),
    last_wait_deadline_ns: std.atomic.Value(i64) = std.atomic.Value(i64).init(-1),
    epoch: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),

    pub fn nowNs(self: *FakeBackend) i96 {
        // safe because: advance publishes the fake clock before waking the scheduler.
        return self.monotonic_ns.load(.acquire);
    }

    pub fn snapshotWake(self: *FakeBackend) u32 {
        // safe because: the epoch detects wakeups only; scheduler state is mutex-protected.
        return self.epoch.load(.monotonic);
    }

    pub fn wait(self: *FakeBackend, seen: u32, deadline_ns: ?i96) void {
        if (deadline_ns) |deadline| self.last_wait_deadline_ns.store(@intCast(deadline), .release);
        common.globalIo().futexWait(u32, &self.epoch.raw, seen) catch |err| switch (err) {
            error.Canceled => {},
        };
    }

    pub fn wake(self: *FakeBackend) void {
        _ = self.epoch.fetchAdd(1, .release); // safe because: waiters only need to observe an epoch change.
        common.globalIo().futexWake(u32, &self.epoch.raw, 1);
    }

    fn advanceMs(self: *FakeBackend, milliseconds: i64) void {
        _ = self.monotonic_ns.fetchAdd(milliseconds * std.time.ns_per_ms, .release); // safe because: wake publishes the new time.
        self.wake();
    }

    fn waitForDeadline(self: *FakeBackend, expected_ns: i64) error{Timeout}!void {
        var waited: u64 = 0;
        while (self.last_wait_deadline_ns.load(.acquire) != expected_ns) {
            if (waited >= TEST_WAIT_NS) return error.Timeout;
            common.sleepNanos(std.time.ns_per_ms);
            waited += std.time.ns_per_ms;
        }
    }
};

const Recorder = struct {
    mutex: common.Mutex = .{},
    values: [MAX_RECORDS]u32 = [_]u32{0} ** MAX_RECORDS,
    count: usize = 0,
    worker_id: ?std.Thread.Id = null,

    const MAX_RECORDS = 256;

    fn record(self: *Recorder, value: u32) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.values[self.count] = value;
        self.count += 1;
        const current = std.Thread.getCurrentId();
        if (self.worker_id) |existing| std.debug.assert(existing == current) else self.worker_id = current;
    }

    fn valueAt(self: *Recorder, index: usize) u32 {
        self.mutex.lock();
        defer self.mutex.unlock();
        return self.values[index];
    }
};

const Blocker = struct {
    entered: common.Event = .{},
    release: common.Event = .{},
};

const TestTarget = struct {
    recorder: *Recorder,
    value: u32,
    blocker: ?*Blocker = null,
    fired: ?*common.Event = null,

    pub fn interrupt(self: TestTarget) InterruptTarget.Outcome {
        if (self.blocker) |blocker| {
            blocker.entered.set();
            blocker.release.timedWait(TEST_WAIT_NS) catch @panic("blocked callback release timed out");
        }
        self.recorder.record(self.value);
        if (self.fired) |fired| fired.set();
        return .interrupted;
    }
};

const TestScheduler = scheduler_module.Scheduler(TestTarget, FakeBackend);
const ProductionScheduler = scheduler_module.Scheduler(TestTarget, scheduler_module.MonotonicBackend);
const TEST_WAIT_NS: u64 = 5 * std.time.ns_per_s;
const BARRIER_PROBE_NS: u64 = 20 * std.time.ns_per_ms;
const REGISTRATION_COUNT = 128;
const CANCEL_COUNT = REGISTRATION_COUNT / 2;

test "production backend reads a nondecreasing boot clock" {
    var backend: scheduler_module.MonotonicBackend = .{};
    const before = backend.nowNs();
    common.sleepNanos(std.time.ns_per_ms);
    try std.testing.expect(backend.nowNs() >= before);
}

test "production backend expires an absolute monotonic deadline" {
    var backend: scheduler_module.MonotonicBackend = .{};
    var recorder: Recorder = .{};
    var fired: common.Event = .{};
    var scheduler = ProductionScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();
    defer scheduler.deinit();

    const guard = try scheduler.arm(.{ .recorder = &recorder, .value = 1, .fired = &fired }, 5);
    for (0..4) |_| backend.wake();
    try fired.timedWait(TEST_WAIT_NS);
    try std.testing.expectEqual(ProductionScheduler.FinishOutcome.fired, guard.finish());
}

test "scheduler preempts wait with monotonic deadline order" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var early_fired: common.Event = .{};
    var late_fired: common.Event = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();
    defer scheduler.deinit();

    const late = try scheduler.arm(.{ .recorder = &recorder, .value = 30, .fired = &late_fired }, 30);
    try backend.waitForDeadline(30 * std.time.ns_per_ms);
    const early = try scheduler.arm(.{ .recorder = &recorder, .value = 5, .fired = &early_fired }, 5);
    try backend.waitForDeadline(5 * std.time.ns_per_ms);
    backend.advanceMs(5);
    try early_fired.timedWait(TEST_WAIT_NS);
    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, early.finish());
    try std.testing.expectEqual(@as(u32, 5), recorder.valueAt(0));

    backend.advanceMs(25);
    try late_fired.timedWait(TEST_WAIT_NS);
    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, late.finish());
    try std.testing.expectEqual(@as(u32, 30), recorder.valueAt(1));
    scheduler.stop();
}

test "deadline guard validates lifecycle" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();
    try std.testing.expectError(error.AlreadyStarted, scheduler.start());

    const guard = try scheduler.arm(.{ .recorder = &recorder, .value = 1 }, 30);
    const copied = guard;
    try std.testing.expectEqual(TestScheduler.FinishOutcome.cancelled, guard.finish());
    try std.testing.expectEqual(TestScheduler.FinishOutcome.already_finished, copied.finish());
    scheduler.stop();
    scheduler.stop();
    try std.testing.expectError(error.SchedulerStopped, scheduler.arm(.{ .recorder = &recorder, .value = 2 }, 1));
    scheduler.deinit();
    scheduler.deinit();
}

test "scheduler can stop before worker start" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);

    try std.testing.expectError(error.SchedulerStopped, scheduler.arm(.{ .recorder = &recorder, .value = 1 }, 1));
    scheduler.stop();
    scheduler.stop();
    try std.testing.expectError(error.SchedulerStopped, scheduler.arm(.{ .recorder = &recorder, .value = 2 }, 1));
    scheduler.deinit();
}

test "deadline finish and stop are quiescence barriers" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var blocker: Blocker = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();

    const guard = try scheduler.arm(.{ .recorder = &recorder, .value = 1, .blocker = &blocker }, 1);
    backend.advanceMs(1);
    try blocker.entered.timedWait(TEST_WAIT_NS);

    var finish_call = FinishCall{ .guard = guard };
    var stop_call = StopCall{ .scheduler = &scheduler };
    const finish_thread = try std.Thread.spawn(.{}, FinishCall.run, .{&finish_call});
    const stop_thread = try std.Thread.spawn(.{}, StopCall.run, .{&stop_call});
    try finish_call.started.timedWait(TEST_WAIT_NS);
    try stop_call.started.timedWait(TEST_WAIT_NS);
    try std.testing.expectError(error.Timeout, finish_call.done.timedWait(BARRIER_PROBE_NS));
    try std.testing.expectError(error.Timeout, stop_call.done.timedWait(BARRIER_PROBE_NS));

    blocker.release.set();
    try finish_call.done.timedWait(TEST_WAIT_NS);
    try stop_call.done.timedWait(TEST_WAIT_NS);
    finish_thread.join();
    stop_thread.join();
    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, finish_call.outcome.?);
    try std.testing.expectEqual(@as(usize, 1), recorder.count);
    scheduler.deinit();
}

test "scheduler stop drains pending registrations after firing callback quiesces" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var blocker: Blocker = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();

    const cancelled = try scheduler.arm(.{ .recorder = &recorder, .value = 9 }, 90);
    try std.testing.expectEqual(TestScheduler.FinishOutcome.cancelled, cancelled.finish());
    const firing = try scheduler.arm(.{ .recorder = &recorder, .value = 1, .blocker = &blocker }, 1);
    const pending_early = try scheduler.arm(.{ .recorder = &recorder, .value = 2 }, 20);
    const pending_late = try scheduler.arm(.{ .recorder = &recorder, .value = 3 }, 30);
    backend.advanceMs(1);
    try blocker.entered.timedWait(TEST_WAIT_NS);

    var stop_call = StopCall{ .scheduler = &scheduler };
    const stop_thread = try std.Thread.spawn(.{}, StopCall.run, .{&stop_call});
    try stop_call.started.timedWait(TEST_WAIT_NS);
    try std.testing.expectError(error.Timeout, stop_call.done.timedWait(BARRIER_PROBE_NS));
    blocker.release.set();
    try stop_call.done.timedWait(TEST_WAIT_NS);
    stop_thread.join();

    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, firing.finish());
    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, pending_early.finish());
    try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, pending_late.finish());
    try std.testing.expectEqual(@as(usize, 3), recorder.count);
    try std.testing.expectEqual(@as(u32, 1), recorder.valueAt(0));
    try std.testing.expectEqual(@as(u32, 2), recorder.valueAt(1));
    try std.testing.expectEqual(@as(u32, 3), recorder.valueAt(2));
    scheduler.deinit();
}

test "scheduler arm frees partial allocations on every failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, exerciseArmAllocation, .{});
}

test "test_scheduler_shutdown_is_bounded_and_leak_free" {
    // Dimension 4.3: shutdown with the full registration mix — pending,
    // mid-fire, and cancelled — joins the worker inside a hard bound and leaves
    // the allocator clean. The production backend runs here on purpose: the
    // bound must hold against the real futex wait, not the fake's manual wake.
    var backend: scheduler_module.MonotonicBackend = .{};
    var recorder: Recorder = .{};
    var blocker: Blocker = .{};
    var scheduler = ProductionScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();

    // Cancelled before shutdown: its registration must already be gone.
    const cancelled = try scheduler.arm(.{ .recorder = &recorder, .value = 9 }, 60_000);
    try std.testing.expectEqual(ProductionScheduler.FinishOutcome.cancelled, cancelled.finish());
    // Mid-fire at shutdown: the callback holds the worker at a barrier so stop
    // provably begins while a target is running.
    const firing = try scheduler.arm(.{ .recorder = &recorder, .value = 1, .blocker = &blocker }, 1);
    // Pending at shutdown: a deadline nothing will ever advance to.
    const pending = try scheduler.arm(.{ .recorder = &recorder, .value = 2 }, 60_000);
    try blocker.entered.timedWait(TEST_WAIT_NS);

    // StopCall is bound to the fake-backend scheduler type; this test stops the
    // production one, so it carries its own thread-shaped stop.
    var stop_call = ProductionStopCall{ .scheduler = &scheduler };
    const stop_thread = try std.Thread.spawn(.{}, ProductionStopCall.run, .{&stop_call});
    try stop_call.started.timedWait(TEST_WAIT_NS);
    blocker.release.set();

    // The bound: stop (drain + callback quiescence) and deinit (worker join)
    // both complete in test time, nowhere near the 60s pending deadline —
    // proving shutdown interrupts the parked wait instead of sleeping it out.
    const started_ns = backend.nowNs();
    try stop_call.done.timedWait(TEST_WAIT_NS);
    stop_thread.join();
    try std.testing.expectEqual(ProductionScheduler.FinishOutcome.fired, firing.finish());
    try std.testing.expectEqual(ProductionScheduler.FinishOutcome.fired, pending.finish());
    scheduler.deinit();
    const elapsed_ns = backend.nowNs() - started_ns;
    try std.testing.expect(elapsed_ns < TEST_WAIT_NS);

    // Quiescent: exactly the drained targets ran, on the (already joined)
    // worker; `testing.allocator` fails the test on any leaked registration.
    try std.testing.expectEqual(@as(usize, 2), recorder.count);
}

test "scheduler concurrency uses one worker and standard ordered tree" {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var start_gate: common.Event = .{};
    var fired = [_]common.Event{.{}} ** REGISTRATION_COUNT;
    var arm_calls: [REGISTRATION_COUNT]ArmCall = undefined;
    var arm_threads: [REGISTRATION_COUNT]std.Thread = undefined;
    var finish_calls: [CANCEL_COUNT]FinishCall = undefined;
    var finish_threads: [CANCEL_COUNT]std.Thread = undefined;
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    try scheduler.start();
    defer scheduler.deinit();

    for (0..REGISTRATION_COUNT) |index| {
        const timeout_ms: u31 = @intCast(REGISTRATION_COUNT - index);
        arm_calls[index] = .{
            .scheduler = &scheduler,
            .start_gate = &start_gate,
            .target = .{ .recorder = &recorder, .value = timeout_ms, .fired = &fired[index] },
            .timeout_ms = timeout_ms,
        };
        arm_threads[index] = try std.Thread.spawn(.{}, ArmCall.run, .{&arm_calls[index]});
    }
    start_gate.set();
    for (arm_threads) |thread| thread.join();
    for (0..CANCEL_COUNT) |index| {
        finish_calls[index] = .{ .guard = arm_calls[index].guard.? };
        finish_threads[index] = try std.Thread.spawn(.{}, FinishCall.run, .{&finish_calls[index]});
    }
    for (finish_threads) |thread| thread.join();
    for (finish_calls) |finish_call| {
        try std.testing.expectEqual(TestScheduler.FinishOutcome.cancelled, finish_call.outcome.?);
    }
    backend.advanceMs(REGISTRATION_COUNT);
    try fired[CANCEL_COUNT].timedWait(TEST_WAIT_NS);

    for (arm_calls[CANCEL_COUNT..]) |arm_call| {
        try std.testing.expectEqual(TestScheduler.FinishOutcome.fired, arm_call.guard.?.finish());
    }
    for (0..REGISTRATION_COUNT - CANCEL_COUNT) |index| {
        try std.testing.expectEqual(@as(u32, @intCast(index + 1)), recorder.valueAt(index));
    }
    try std.testing.expectEqual(@as(usize, REGISTRATION_COUNT - CANCEL_COUNT), recorder.count);
    try std.testing.expect(recorder.worker_id != null);
    scheduler.stop();
}

const FinishCall = struct {
    guard: TestScheduler.Guard,
    started: common.Event = .{},
    done: common.Event = .{},
    outcome: ?TestScheduler.FinishOutcome = null,

    fn run(self: *FinishCall) void {
        self.started.set();
        self.outcome = self.guard.finish();
        self.done.set();
    }
};

const StopCall = struct {
    scheduler: *TestScheduler,
    started: common.Event = .{},
    done: common.Event = .{},

    fn run(self: *StopCall) void {
        self.started.set();
        self.scheduler.stop();
        self.done.set();
    }
};

/// StopCall for the production-backend scheduler (the 4.3 shutdown test).
const ProductionStopCall = struct {
    scheduler: *ProductionScheduler,
    started: common.Event = .{},
    done: common.Event = .{},

    fn run(self: *ProductionStopCall) void {
        self.started.set();
        self.scheduler.stop();
        self.done.set();
    }
};

const ArmCall = struct {
    scheduler: *TestScheduler,
    start_gate: *common.Event,
    target: TestTarget,
    timeout_ms: u31,
    guard: ?TestScheduler.Guard = null,

    fn run(self: *ArmCall) void {
        self.start_gate.timedWait(TEST_WAIT_NS) catch @panic("arm start gate timed out");
        self.guard = self.scheduler.arm(self.target, self.timeout_ms) catch |err| {
            std.debug.panic("scheduler arm failed: {s}", .{@errorName(err)});
        };
    }
};

fn exerciseArmAllocation(alloc: std.mem.Allocator) !void {
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var scheduler = TestScheduler.init(alloc, &backend);
    try scheduler.start();
    defer scheduler.deinit();
    const guard = try scheduler.arm(.{ .recorder = &recorder, .value = 1 }, 1);
    _ = guard.finish();
    scheduler.stop();
}

/// Concurrent registrations held open to force the pool to retain several nodes.
const REUSE_POOL_DEPTH: usize = 8;
/// Long enough that no arm in the reuse test can fire before it is cancelled.
const REUSE_ARM_TIMEOUT_MS: u31 = 60_000;
/// Enough cycles that any per-arm allocation would be unmistakable.
const REUSE_CYCLES: usize = 64;

test "steady-state arm and finish reuse registrations without touching the allocator" {
    // Allocation churn under the one mutex every owner contends for is the
    // scale trap here: the reuse pool must make a repeated arm/finish pointer
    // work, so a future per-request caller cannot serialize on the allocator.
    var counting = std.testing.FailingAllocator.init(std.testing.allocator, .{});
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var scheduler = TestScheduler.init(counting.allocator(), &backend);
    defer scheduler.deinit();
    try scheduler.start();

    // Warm up: the first arm allocates its registration and sizes the map.
    var warmup = try scheduler.arm(.{ .recorder = &recorder, .value = 1 }, REUSE_ARM_TIMEOUT_MS);
    try std.testing.expectEqual(TestScheduler.FinishOutcome.cancelled, warmup.finish());
    const warmed_allocations = counting.allocations;

    var cycle: usize = 0;
    while (cycle < REUSE_CYCLES) : (cycle += 1) {
        var guard = try scheduler.arm(.{ .recorder = &recorder, .value = 2 }, REUSE_ARM_TIMEOUT_MS);
        try std.testing.expectEqual(TestScheduler.FinishOutcome.cancelled, guard.finish());
    }
    // Zero growth: every cycle after the first came from the reuse pool.
    try std.testing.expectEqual(warmed_allocations, counting.allocations);
}

test "the reuse pool is freed by deinit, not leaked across arms" {
    // Concurrent depth forces several registrations to exist at once, so the
    // pool retains more than one node; testing.allocator fails the test if any
    // of them outlives deinit.
    var backend: FakeBackend = .{};
    var recorder: Recorder = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);
    defer scheduler.deinit();
    try scheduler.start();

    var guards: [REUSE_POOL_DEPTH]TestScheduler.Guard = undefined;
    for (&guards, 0..) |*guard, index| {
        guard.* = try scheduler.arm(.{ .recorder = &recorder, .value = @intCast(index) }, REUSE_ARM_TIMEOUT_MS);
    }
    for (&guards) |*guard| _ = guard.finish();
    // All eight are now pooled; a second round must consume them, not allocate.
    for (&guards, 0..) |*guard, index| {
        guard.* = try scheduler.arm(.{ .recorder = &recorder, .value = @intCast(index) }, REUSE_ARM_TIMEOUT_MS);
    }
    for (&guards) |*guard| _ = guard.finish();
}
