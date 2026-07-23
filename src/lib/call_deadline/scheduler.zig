//! Process-owned monotonic deadline scheduler.

const std = @import("std");
const common = @import("common");
const logging = @import("log");
const InterruptTarget = @import("InterruptTarget.zig");
const log = logging.scoped(.call_deadline);

/// The scheduler every production network owner arms against: owner-mediated
/// interruption plus the boot clock.
pub const ProcessScheduler = Scheduler(InterruptTarget, MonotonicBackend);

pub const MonotonicBackend = struct {
    epoch: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),

    pub fn nowNs(_: *MonotonicBackend) i96 {
        return std.Io.Clock.boot.now(common.globalIo()).toNanoseconds();
    }

    pub fn snapshotWake(self: *MonotonicBackend) u32 {
        // safe because: the epoch detects wakeups only; scheduler state is mutex-protected.
        return self.epoch.load(.monotonic);
    }

    pub fn wait(self: *MonotonicBackend, seen: u32, deadline_ns: ?i96) void {
        const io = common.globalIo();
        const timeout: std.Io.Timeout = if (deadline_ns) |deadline| blk: {
            if (deadline <= self.nowNs()) return;
            break :blk .{ .deadline = std.Io.Timestamp.fromNanoseconds(deadline).withClock(.boot) };
        } else .none;
        io.futexWaitTimeout(u32, &self.epoch.raw, seen, timeout) catch |err| switch (err) {
            error.Canceled => {},
        };
    }

    pub fn wake(self: *MonotonicBackend) void {
        _ = self.epoch.fetchAdd(1, .release); // safe because: waiters only need to observe an epoch change.
        common.globalIo().futexWake(u32, &self.epoch.raw, 1);
    }
};

const StdThreadSpawner = struct {
    fn spawn(comptime entry: anytype, args: anytype) std.Thread.SpawnError!std.Thread {
        return std.Thread.spawn(.{}, entry, args);
    }
};

/// `Target.interrupt` executes on the sole worker, returns an
/// `InterruptTarget.Outcome`, and must be a bounded, nonblocking leaf
/// operation. It must not call scheduler barriers.
pub fn Scheduler(comptime Target: type, comptime Backend: type) type {
    return SchedulerWithSpawner(Target, Backend, StdThreadSpawner);
}

fn SchedulerWithSpawner(comptime Target: type, comptime Backend: type, comptime Spawner: type) type {
    return struct {
        const Self = @This();
        const DeadlineTree = std.Treap(DeadlineKey, compareDeadline);

        alloc: std.mem.Allocator,
        backend: *Backend,
        /// Guards deadlines, registrations, lifecycle state, worker handle, and identifier allocation.
        mutex: common.Mutex = .{},
        cond: common.Condition = .{},
        deadlines: DeadlineTree = .{},
        registrations: std.AutoHashMapUnmanaged(u64, *Registration) = .empty,
        next_id: u64 = 1,
        thread: ?std.Thread = null,
        state: State = .initialized,

        pub const ArmError = error{ SchedulerStopped, IdentifierExhausted, OutOfMemory };
        pub const StartError = std.Thread.SpawnError || error{AlreadyStarted};
        pub const FinishOutcome = enum { cancelled, fired, already_finished };

        const State = enum { initialized, running, stopping, stopped, deinitialized };
        const RegistrationState = enum { pending, firing, fired };
        const DeadlineKey = struct { deadline_ns: i96, id: u64 };
        const Registration = struct {
            node: DeadlineTree.Node,
            target: Target,
            state: RegistrationState = .pending,
        };
        const Fire = struct { id: u64, target: Target };
        const Next = union(enum) { wait: ?i96, fire: Fire, exit };

        pub const Guard = struct {
            scheduler: *Self,
            id: u64,

            pub fn finish(self: Guard) FinishOutcome {
                return self.scheduler.finish(self.id);
            }
        };
        const ArmResult = struct { guard: Guard, wake_worker: bool };

        pub fn init(alloc: std.mem.Allocator, backend: *Backend) Self {
            return .{ .alloc = alloc, .backend = backend };
        }

        /// Caller must keep `self` address-stable until `deinit` returns.
        pub fn start(self: *Self) StartError!void {
            self.mutex.lock();
            defer self.mutex.unlock();
            if (self.state != .initialized) return error.AlreadyStarted;
            self.state = .running;
            self.thread = Spawner.spawn(workerMain, .{self}) catch |err| {
                self.state = .initialized;
                log.err("scheduler_start_failed", .{ .err = @errorName(err) });
                return err;
            };
        }

        pub fn arm(self: *Self, target: Target, timeout_ms: u31) ArmError!Guard {
            self.mutex.lock();
            const result = self.armLocked(target, timeout_ms) catch |err| {
                self.mutex.unlock();
                return err;
            };
            self.mutex.unlock();
            if (result.wake_worker) self.backend.wake();
            return result.guard;
        }

        fn armLocked(self: *Self, target: Target, timeout_ms: u31) ArmError!ArmResult {
            if (self.state != .running) {
                log.debug("deadline_arm_rejected", .{ .state = @tagName(self.state) });
                return error.SchedulerStopped;
            }
            if (self.next_id == std.math.maxInt(u64)) return error.IdentifierExhausted;

            const id = self.next_id;
            const deadline_ns = self.backend.nowNs() + @as(i96, timeout_ms) * std.time.ns_per_ms;
            const key: DeadlineKey = .{ .deadline_ns = deadline_ns, .id = id };
            const previous_min = self.deadlines.getMin();
            const registration = try self.alloc.create(Registration);
            errdefer self.alloc.destroy(registration);
            // SAFETY: Treap entry insertion initializes every intrusive node link before use.
            registration.* = .{ .node = undefined, .target = target };
            try self.registrations.putNoClobber(self.alloc, id, registration);
            errdefer _ = self.registrations.remove(id);
            var entry = self.deadlines.getEntryFor(key);
            entry.set(&registration.node);
            self.next_id += 1;
            const wake_worker = previous_min == null or compareDeadline(key, previous_min.?.key) == .lt;
            return .{ .guard = .{ .scheduler = self, .id = id }, .wake_worker = wake_worker };
        }

        pub fn stop(self: *Self) void {
            self.mutex.lock();
            switch (self.state) {
                .initialized => {
                    self.state = .stopped;
                    self.mutex.unlock();
                    return;
                },
                .running => self.state = .stopping,
                .stopping => {
                    while (self.state == .stopping) self.cond.wait(&self.mutex);
                    self.mutex.unlock();
                    return;
                },
                .stopped, .deinitialized => {
                    self.mutex.unlock();
                    return;
                },
            }
            const worker = self.thread.?;
            self.thread = null;
            self.mutex.unlock();

            self.backend.wake();
            worker.join();
            self.mutex.lock();
            self.state = .stopped;
            log.debug("scheduler_stopped", .{ .registration_count = self.registrations.count() });
            self.cond.broadcast();
            self.mutex.unlock();
        }

        /// All threads that may call `arm`, `finish`, or `stop` must be joined
        /// before `deinit` begins; `deinit` is the exclusive lifetime barrier.
        pub fn deinit(self: *Self) void {
            // discipline: ok — the terminal state and emptied map deliberately
            // preserve harmless copied-guard and repeated-deinit behavior.
            self.mutex.lock();
            if (self.state == .deinitialized) {
                self.mutex.unlock();
                return;
            }
            self.mutex.unlock();
            self.stop();

            self.mutex.lock();
            defer self.mutex.unlock();
            std.debug.assert(self.deadlines.root == null);
            var registrations = self.registrations.valueIterator();
            while (registrations.next()) |registration| self.alloc.destroy(registration.*);
            self.registrations.deinit(self.alloc);
            self.registrations = .empty;
            self.state = .deinitialized;
        }

        fn finish(self: *Self, id: u64) FinishOutcome {
            self.mutex.lock();
            defer self.mutex.unlock();
            while (true) {
                const registration = self.registrations.get(id) orelse return .already_finished;
                switch (registration.state) {
                    .pending => return self.removeRegistration(id, registration, .cancelled),
                    .firing => self.cond.wait(&self.mutex),
                    .fired => return self.removeRegistration(id, registration, .fired),
                }
            }
        }

        fn removeRegistration(self: *Self, id: u64, registration: *Registration, outcome: FinishOutcome) FinishOutcome {
            if (registration.state == .pending) {
                var entry = self.deadlines.getEntryForExisting(&registration.node);
                entry.set(null);
            }
            _ = self.registrations.remove(id);
            self.alloc.destroy(registration);
            return outcome;
        }

        fn workerMain(self: *Self) void {
            while (true) {
                self.mutex.lock();
                const next = self.nextLocked();
                const seen = self.backend.snapshotWake();
                self.mutex.unlock();
                switch (next) {
                    .exit => return,
                    .wait => |deadline| self.backend.wait(seen, deadline),
                    .fire => |selected| self.fire(selected),
                }
            }
        }

        fn nextLocked(self: *Self) Next {
            const node = self.deadlines.getMin() orelse {
                return if (self.state == .stopping) .exit else .{ .wait = null };
            };
            if (self.state == .running and self.backend.nowNs() < node.key.deadline_ns) {
                return .{ .wait = node.key.deadline_ns };
            }
            const id = node.key.id;
            var entry = self.deadlines.getEntryForExisting(node);
            entry.set(null);
            const registration = self.registrations.get(id).?;
            registration.state = .firing;
            return .{ .fire = .{ .id = id, .target = registration.target } };
        }

        fn fire(self: *Self, selected: Fire) void {
            // A `stale` outcome is the healthy answer to a late fire: the owner
            // had already replaced or completed that connection generation and
            // touched nothing. Logging it keeps that distinguishable from a
            // real interruption without adding a metric.
            const outcome = selected.target.interrupt();
            log.debug("deadline_fired", .{ .outcome = @tagName(outcome) });
            self.mutex.lock();
            defer self.mutex.unlock();
            if (self.registrations.get(selected.id)) |registration| registration.state = .fired;
            self.cond.broadcast();
        }

        fn compareDeadline(a: DeadlineKey, b: DeadlineKey) std.math.Order {
            const deadline_order = std.math.order(a.deadline_ns, b.deadline_ns);
            return if (deadline_order == .eq) std.math.order(a.id, b.id) else deadline_order;
        }
    };
}

const FailingThreadSpawner = struct {
    fn spawn(comptime entry: anytype, args: anytype) std.Thread.SpawnError!std.Thread {
        _ = entry;
        _ = args;
        return error.ThreadQuotaExceeded;
    }
};

const StartFailureBackend = struct {
    fn nowNs(_: *StartFailureBackend) i96 {
        return 0;
    }

    fn snapshotWake(_: *StartFailureBackend) u32 {
        return 0;
    }

    fn wait(_: *StartFailureBackend, _: u32, _: ?i96) void {}
    fn wake(_: *StartFailureBackend) void {}
};

const StartFailureTarget = struct {
    fn interrupt(_: StartFailureTarget) InterruptTarget.Outcome {
        return .stale;
    }
};

test "scheduler start failure resets state and remains fail closed" {
    const TestScheduler = SchedulerWithSpawner(StartFailureTarget, StartFailureBackend, FailingThreadSpawner);
    var backend: StartFailureBackend = .{};
    var scheduler = TestScheduler.init(std.testing.allocator, &backend);

    try std.testing.expectError(error.ThreadQuotaExceeded, scheduler.start());
    try std.testing.expectError(error.SchedulerStopped, scheduler.arm(.{}, 1));
    try std.testing.expectError(error.ThreadQuotaExceeded, scheduler.start());
    scheduler.deinit();
}
