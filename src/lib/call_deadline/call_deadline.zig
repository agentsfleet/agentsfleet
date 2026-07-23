//! Call-bounding policy + mechanism shared by both build graphs (the
//! `call_deadline` named module): the runner control-plane per-verb deadline
//! defaults (env-overridable via the runner's config.zig) and the watchdog
//! that enforces a deadline on an in-flight HTTP call. The watchdog shuts the
//! in-flight pooled socket down at the deadline — the portable way to wake a
//! blocked read on the threaded Io, whose recv path treats a SO_RCVTIMEO
//! EAGAIN as a programmer bug.
//!
//! `Watchdog(log_spec)` is comptime-parameterized (the `std.log.scoped`
//! idiom) so each consumer keeps its own log identity: the runner's
//! control-plane client bakes in its shipped `cp_*` events + transport-loss
//! error code, while the daemon's connector `bounded_fetch` instantiates
//! `Watchdog(null)` — a silent mechanism — and logs with request context
//! (provider, call class) one layer up.

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");

// Call-site deadlines. The required parameter on every client verb is the
// compile-time guarantee that no control-plane call is unbounded; only
// deadlines with a distinct rationale get their own name.
/// Default verb deadline (heartbeat, lease poll, self, memory hydrate/capture).
pub const DEFAULT_DEADLINE_MS: u31 = 10_000;
/// Reports carry the full response_text + checkpoint payload — extra headroom.
pub const REPORT_DEADLINE_MS: u31 = 15_000;
/// Live-tail batches are best-effort; tight bound so a dead control plane
/// cannot stall the frame pump for long.
pub const ACTIVITY_DEADLINE_MS: u31 = 5_000;
/// Renewal carries the kill-path invariant (comptime relation below): a hung
/// control plane delays the child's deadline kill by at most this bound, and
/// a failed bounded attempt still leaves room for one retry tick inside the
/// renewal window.
pub const RENEW_DEADLINE_MS: u31 = 4_000;

comptime {
    // First renew attempt fires ~RENEWAL_WINDOW_MS before expiry; if it blocks
    // for the full bound and fails, the next tick (RENEWAL_TICK_MS later) must
    // still start a retry before the lease expires. Env overrides are
    // re-clamped against the same relation at config load.
    std.debug.assert(RENEW_DEADLINE_MS + common.RENEWAL_TICK_MS < common.RENEWAL_WINDOW_MS);
}

// The owner-safe deadline mechanism. Consumers reach these through the
// `call_deadline` named module; the leaf files are not relative-importable from
// another module's tree.
pub const scheduler = @import("scheduler.zig");
pub const InterruptTarget = @import("InterruptTarget.zig");
pub const SocketOwner = @import("SocketOwner.zig");
/// One per process, owned by the process root and passed to every network owner.
pub const ProcessScheduler = scheduler.ProcessScheduler;
pub const MonotonicBackend = scheduler.MonotonicBackend;

/// The resolved per-verb deadlines a runner daemon runs with. Defaults are the
/// consts above; the runner's `config.zig` overrides them from the environment
/// (clamped, renew strictly inside the renewal-window relation).
pub const Deadlines = struct {
    default_ms: u31 = DEFAULT_DEADLINE_MS,
    report_ms: u31 = REPORT_DEADLINE_MS,
    activity_ms: u31 = ACTIVITY_DEADLINE_MS,
    renew_ms: u31 = RENEW_DEADLINE_MS,
};

/// Granularity of the watchdog's deadline checks (also its disarm latency).
const POLL_SLICE_MS: i64 = 50;

/// Result of `arm`: either the deadline is being enforced, or the watchdog
/// thread could not be established. The caller MUST treat
/// `.watchdog_unavailable` as fatal for that verb — running the call unbounded
/// is the silent-hang the watchdog exists to prevent.
pub const ArmOutcome = enum { armed, watchdog_unavailable };

comptime {
    // Make the watchdog's cross-thread correctness EXPLICIT instead of
    // accidental. `common.Mutex`/`Condition` wrap `std.Io.Mutex`, whose
    // blocking path is real atomics + an OS futex — but `Thread.futexWait`/`Wake`
    // degrade to `unreachable`/no-op under a `single_threaded` BUILD. The
    // watchdog runs on its own `std.Thread`, so a single-threaded build would
    // silently break the deadline. (Zig 0.16 removed `std.Thread.Mutex`; the
    // Io-backed primitive IS the thread-safe lock here — provided this holds.)
    std.debug.assert(!builtin.single_threaded);
}

/// Comptime log identity for a `Watchdog` instantiation — baked in so the
/// scoped logger and event names cost nothing at runtime. `null` keeps the
/// watchdog silent; the owner then logs from its own context, with fields the
/// mechanism cannot know (which provider / call class was in flight).
pub const LogSpec = struct {
    scope: @TypeOf(.enum_literal),
    /// warn emitted from the watchdog thread when a deadline fires.
    fire_event: []const u8,
    /// err emitted when the lazy thread spawn fails (the call is refused).
    spawn_fail_event: []const u8,
    /// `error_code=` field value on both events.
    error_code: []const u8,
};

/// Best-effort `shutdown(2)` on a raw socket fd, no libc required. On Linux we
/// issue the syscall directly (`std.os.linux.shutdown`) so this module compiles
/// in a test graph that doesn't link libc — the agentsfleetd/runner binaries
/// link it, but the standalone `test-lib` compilation does not, and `std.c`
/// there is a compile error. macOS has no stable syscall ABI and always links
/// libc, so it keeps the `std.c` path.
pub fn shutdownSocket(handle: std.posix.fd_t) void {
    if (comptime builtin.os.tag == .linux) {
        _ = std.os.linux.shutdown(handle, std.os.linux.SHUT.RDWR);
    } else {
        _ = std.c.shutdown(handle, std.c.SHUT.RDWR);
    }
}

/// One watchdog per single-call-at-a-time client context. While a call is
/// armed, a deadline pass shuts the in-flight socket down, waking the blocked
/// read; the verb surfaces a retryable transport error and the pool replaces
/// the dead connection on the next call. The thread spawns lazily on first arm
/// and is joined by deinit (its wake path: the exit flag + condition signal).
pub fn Watchdog(comptime log_spec: ?LogSpec) type {
    return struct {
        const Self = @This();

        // `mutex`/`cond` are the futex-backed `common.Mutex`/`Condition`
        // (`std.Io.Mutex`): genuinely cross-thread under the multi-threaded
        // build the comptime guard above enforces. The watchdog runs on its
        // own OS thread while the client thread arms/disarms it.
        mutex: common.Mutex = .{},
        cond: common.Condition = .{},
        thread: ?std.Thread = null,
        exit: bool = false,
        armed: bool = false,
        // SAFETY: written by arm() before armed=true; read only while armed.
        handle: std.Io.net.Socket.Handle = undefined,
        deadline_at_ms: i64 = 0,
        /// True iff the most recent armed call was shut down by its deadline;
        /// cleared by the next `arm`. Read via `deadlineFired`.
        fired: bool = false,
        /// Test-only: force the lazy spawn to fail so the
        /// `.watchdog_unavailable` path is exercisable (a real
        /// `std.Thread.spawn` failure can't be induced). Comptime-dead in
        /// release — the gate below folds away.
        force_spawn_fail_for_test: bool = false,

        pub fn arm(self: *Self, handle: std.Io.net.Socket.Handle, deadline_ms: u31) ArmOutcome {
            self.mutex.lock();
            if (self.thread == null and !self.exit) {
                const spawned: ?std.Thread = if (builtin.is_test and self.force_spawn_fail_for_test)
                    null
                else
                    std.Thread.spawn(.{}, loop, .{self}) catch null;
                self.thread = spawned orelse {
                    // No watchdog thread → refuse the call rather than run it
                    // unbounded. Persistent failure (thread exhaustion) makes
                    // the verb fail fast + loud, never a silent hang.
                    self.mutex.unlock();
                    if (comptime log_spec) |spec|
                        logging.scoped(spec.scope).err(spec.spawn_fail_event, .{ .error_code = spec.error_code });
                    return .watchdog_unavailable;
                };
            }
            self.handle = handle;
            self.deadline_at_ms = clock.nowMillis() + deadline_ms;
            self.fired = false;
            self.armed = true;
            self.mutex.unlock();
            self.cond.signal();
            return .armed;
        }

        pub fn disarm(self: *Self) void {
            self.mutex.lock();
            defer self.mutex.unlock();
            self.armed = false;
        }

        /// True iff the most recent armed call hit its deadline — lets the
        /// owner tell a deadline fire from an ordinary vendor transport
        /// failure after the verb returns. Stays true through `disarm`;
        /// cleared by the next `arm`.
        pub fn deadlineFired(self: *Self) bool {
            self.mutex.lock();
            defer self.mutex.unlock();
            return self.fired;
        }

        pub fn deinit(self: *Self) void {
            self.mutex.lock();
            self.exit = true;
            self.armed = false;
            self.mutex.unlock();
            self.cond.signal();
            if (self.thread) |t| t.join();
            self.thread = null;
        }

        fn loop(self: *Self) void {
            self.mutex.lock();
            while (!self.exit) {
                if (!self.armed) {
                    // Woken by arm() or deinit(); the predicate is re-checked
                    // under the mutex (both mutate it locked, so no lost wakeup).
                    self.cond.wait(&self.mutex);
                    continue;
                }
                const now = clock.nowMillis();
                if (now >= self.deadline_at_ms) {
                    // Fire UNDER the lock: a completed call's disarm + a
                    // successor call's arm (recycling the same fd number from
                    // the pool) can otherwise interleave between the check and
                    // the syscall and the shutdown would hit the next call's
                    // socket. shutdown(2) is non-blocking; the hold is
                    // microseconds.
                    shutdownSocket(self.handle);
                    self.armed = false;
                    self.fired = true;
                    if (comptime log_spec) |spec| {
                        self.mutex.unlock();
                        logging.scoped(spec.scope).warn(spec.fire_event, .{ .error_code = spec.error_code });
                        self.mutex.lock();
                    }
                    continue;
                }
                // Bounded slice sleep outside the lock, then re-check: a disarm
                // during the slice means the fire branch is never reached.
                const slice_ms = @min(POLL_SLICE_MS, self.deadline_at_ms - now);
                self.mutex.unlock();
                common.sleepNanos(@intCast(slice_ms * std.time.ns_per_ms));
                self.mutex.lock();
            }
            self.mutex.unlock();
        }
    };
}

// ── Tests ───────────────────────────────────────────────────────────────────
// The log identity is comptime data, so one representative spec exercises the
// generic exactly as consumers instantiate it. The error_code field is a log
// field value, not a wire code — kept non-`UZ-` so the registry orphan audit
// stays authoritative for real codes.
const TestWatchdog = Watchdog(.{
    .scope = .call_deadline,
    .fire_event = "call_deadline_fired",
    .spawn_fail_event = "call_watchdog_spawn_failed",
    .error_code = "test-transport-loss",
});

test "watchdog lock is the futex-backed cross-thread primitive under a multi-threaded build" {
    // Zig 0.16 has no std.Thread.Mutex; the thread-safe lock here is the
    // futex-backed common.Mutex (std.Io.Mutex). Its cross-thread correctness
    // requires a multi-threaded build — the comptime guard in this file enforces
    // that, and this asserts the invariant + the lock type a regression would
    // have to break.
    const wd: TestWatchdog = .{};
    try std.testing.expect(@TypeOf(wd.mutex) == common.Mutex);
    try std.testing.expect(@TypeOf(wd.cond) == common.Condition);
    try std.testing.expect(!builtin.single_threaded);
}

test "watchdog mutex actually serializes two real threads (no lost updates)" {
    // Functional proof the lock is genuinely cross-thread: two OS threads each
    // bump a shared counter under the watchdog's mutex; a non-serializing lock
    // would lose updates to the race.
    const BUMPS_PER_THREAD: u64 = 50_000;
    const Shared = struct {
        mutex: common.Mutex = .{},
        counter: u64 = 0,
        fn bump(self: *@This()) void {
            for (0..BUMPS_PER_THREAD) |_| {
                self.mutex.lock();
                self.counter += 1;
                self.mutex.unlock();
            }
        }
    };
    var s: Shared = .{};
    const t0 = try std.Thread.spawn(.{}, Shared.bump, .{&s});
    const t1 = try std.Thread.spawn(.{}, Shared.bump, .{&s});
    t0.join();
    t1.join();
    try std.testing.expectEqual(2 * BUMPS_PER_THREAD, s.counter);
}

test "watchdog spawn failure is fatal/observable, never silent-unbounded" {
    // Forced spawn failure → arm reports .watchdog_unavailable (the caller turns
    // this into a failed verb) and leaves the watchdog disarmed with no thread —
    // the call is refused, not run without a deadline. Proven for both the
    // logging and the silent instantiation (the refuse path is log-agnostic).
    var wd: TestWatchdog = .{ .force_spawn_fail_for_test = true };
    defer wd.deinit();
    const outcome = wd.arm(0, DEFAULT_DEADLINE_MS);
    try std.testing.expectEqual(ArmOutcome.watchdog_unavailable, outcome);
    try std.testing.expect(!wd.armed);
    try std.testing.expect(wd.thread == null);

    // Recovery: once the forced-failure flag clears, a subsequent arm succeeds
    // and spins up the real thread (proves the failure path didn't wedge state).
    wd.force_spawn_fail_for_test = false;
    try std.testing.expectEqual(ArmOutcome.armed, wd.arm(0, DEFAULT_DEADLINE_MS));
    try std.testing.expect(wd.armed);
    wd.disarm();

    var silent: Watchdog(null) = .{ .force_spawn_fail_for_test = true };
    defer silent.deinit();
    try std.testing.expectEqual(ArmOutcome.watchdog_unavailable, silent.arm(0, DEFAULT_DEADLINE_MS));
}

test "deadlineFired: cleared by arm, survives disarm (the owner reads it post-verb)" {
    var wd: Watchdog(null) = .{};
    defer wd.deinit();
    // Simulate a prior fire, then prove the next arm clears it (the flag is
    // per-call state, never sticky across calls).
    wd.fired = true;
    try std.testing.expect(wd.deadlineFired());
    try std.testing.expectEqual(ArmOutcome.armed, wd.arm(0, DEFAULT_DEADLINE_MS));
    try std.testing.expect(!wd.deadlineFired());
    // A clean (non-fired) call keeps it false through disarm.
    wd.disarm();
    try std.testing.expect(!wd.deadlineFired());
}

test {
    _ = @import("scheduler_test.zig");
    _ = InterruptTarget;
    _ = SocketOwner;
}
