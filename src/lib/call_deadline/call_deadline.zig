//! Call-bounding policy + mechanism shared by both build graphs (the
//! `call_deadline` named module): the runner control-plane per-verb deadline
//! defaults (env-overridable via the runner's config.zig) and the owner-safe
//! deadline mechanism every network caller arms against.
//!
//! Each PROCESS owns exactly one `ProcessScheduler`, created by its root and
//! passed explicitly to every network owner — there is no hidden global and no
//! per-call watchdog thread. A registration names a `SocketOwner` generation,
//! never a descriptor number, so a late fire against a replaced connection is
//! provably a no-op instead of a cross-connection kill. Shutting the in-flight
//! socket down remains the portable way to wake a blocked read on the threaded
//! Io, whose recv path treats a SO_RCVTIMEO EAGAIN as a programmer bug.
//!
//! Logging belongs to the OWNER, not the mechanism: the scheduler core emits
//! debug-level mechanism events, and each caller (the runner control-plane
//! client, the daemon's connector `bounded_fetch`) emits the visible warn/error
//! with the request context — provider, call class — the worker never sees.

const std = @import("std");
const builtin = @import("builtin");
const common = @import("common");

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

comptime {
    // Make the scheduler's cross-thread correctness EXPLICIT instead of
    // accidental. `common.Mutex`/`Condition` wrap `std.Io.Mutex`, whose
    // blocking path is real atomics + an OS futex — but `Thread.futexWait`/`Wake`
    // degrade to `unreachable`/no-op under a `single_threaded` BUILD. The
    // scheduler worker runs on its own `std.Thread`, so a single-threaded build
    // would silently break every deadline. (Zig 0.16 removed `std.Thread.Mutex`;
    // the Io-backed primitive IS the thread-safe lock — provided this holds.)
    std.debug.assert(!builtin.single_threaded);
}

test {
    _ = @import("scheduler_test.zig");
    _ = @import("migration_audit_test.zig");
    _ = InterruptTarget;
    _ = SocketOwner;
}
