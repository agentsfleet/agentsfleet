//! Call-bounding policy + mechanism for the control-plane client: the per-verb
//! deadline defaults (env-overridable via config.zig) and the watchdog that
//! enforces them. The watchdog shuts the in-flight pooled socket down at the
//! deadline — the portable way to wake a blocked read on the threaded Io,
//! whose recv path treats a SO_RCVTIMEO EAGAIN as a programmer bug.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const logging = @import("log");
const client_errors = @import("../engine/client_errors.zig");

const log = logging.scoped(.fleet_runner);
const ERR_EXEC_TRANSPORT_LOSS = client_errors.ERR_EXEC_TRANSPORT_LOSS;

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

/// The resolved per-verb deadlines a daemon runs with. Defaults are the consts
/// above; `config.zig` overrides them from the environment (clamped, renew
/// strictly inside the renewal-window relation).
pub const Deadlines = struct {
    default_ms: u31 = DEFAULT_DEADLINE_MS,
    report_ms: u31 = REPORT_DEADLINE_MS,
    activity_ms: u31 = ACTIVITY_DEADLINE_MS,
    renew_ms: u31 = RENEW_DEADLINE_MS,
};

/// Granularity of the watchdog's deadline checks (also its disarm latency).
const POLL_SLICE_MS: i64 = 50;

/// One watchdog per client. While a call is armed, a deadline pass shuts the
/// in-flight socket down, waking the blocked read; the verb surfaces a
/// retryable transport error and the pool replaces the dead connection on the
/// next call. The thread spawns lazily on first arm and is joined by deinit
/// (its wake path: the exit flag + condition signal).
pub const CallWatchdog = struct {
    mutex: common.Mutex = .{},
    cond: common.Condition = .{},
    thread: ?std.Thread = null,
    exit: bool = false,
    armed: bool = false,
    // SAFETY: written by arm() before armed=true; read only while armed.
    handle: std.Io.net.Socket.Handle = undefined,
    deadline_at_ms: i64 = 0,

    pub fn arm(self: *CallWatchdog, handle: std.Io.net.Socket.Handle, deadline_ms: u31) void {
        self.mutex.lock();
        if (self.thread == null and !self.exit) {
            self.thread = std.Thread.spawn(.{}, loop, .{self}) catch blk: {
                // No watchdog thread → this call runs unbounded; visible, rare.
                log.warn("cp_watchdog_spawn_failed", .{});
                break :blk null;
            };
        }
        self.handle = handle;
        self.deadline_at_ms = clock.nowMillis() + deadline_ms;
        self.armed = true;
        self.mutex.unlock();
        self.cond.signal();
    }

    pub fn disarm(self: *CallWatchdog) void {
        self.mutex.lock();
        defer self.mutex.unlock();
        self.armed = false;
    }

    pub fn deinit(self: *CallWatchdog) void {
        self.mutex.lock();
        self.exit = true;
        self.armed = false;
        self.mutex.unlock();
        self.cond.signal();
        if (self.thread) |t| t.join();
        self.thread = null;
    }

    fn loop(self: *CallWatchdog) void {
        self.mutex.lock();
        while (!self.exit) {
            if (!self.armed) {
                // Woken by arm() or deinit(); the predicate is re-checked under
                // the mutex (both mutate it locked, so no lost wakeup).
                self.cond.wait(&self.mutex);
                continue;
            }
            const now = clock.nowMillis();
            if (now >= self.deadline_at_ms) {
                // Fire UNDER the lock: a completed call's disarm + a successor
                // call's arm (recycling the same fd number from the pool) can
                // otherwise interleave between the check and the syscall and
                // the shutdown would hit the next call's socket. shutdown(2)
                // is non-blocking; the hold is microseconds.
                _ = std.c.shutdown(self.handle, std.c.SHUT.RDWR);
                self.armed = false;
                self.mutex.unlock();
                log.warn("cp_call_deadline_fired", .{ .error_code = ERR_EXEC_TRANSPORT_LOSS });
                self.mutex.lock();
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
