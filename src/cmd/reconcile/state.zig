//! Shared daemon state types and globals for the reconcile submodules.
//!
//! Isolated into its own file to break the import cycle that would arise if
//! daemon.zig and emit.zig both imported each other.
//!
//! Ownership:
//!   - `g_daemon_state` is a raw pointer to a stack-allocated `DaemonState`
//!     inside `runDaemon`.  It is set before the daemon loop starts and
//!     cleared via `defer` when `runDaemon` returns.  All readers must tolerate
//!     a null value.

const std = @import("std");

pub const DaemonState = struct {
    alloc: std.mem.Allocator,
    interval_seconds: u64,
    started_ms: i64,
    running: std.atomic.Value(bool),
    last_attempt_ms: std.atomic.Value(i64),
    last_success_ms: std.atomic.Value(i64),
    last_dead_lettered: std.atomic.Value(u32),
    total_ticks: std.atomic.Value(u64),
    consecutive_failures: std.atomic.Value(u32),
};

pub var g_daemon_state: ?*DaemonState = null;

/// Returns true iff the daemon state indicates a recent healthy tick.
/// Checks: running, no consecutive failures, and last success within interval*3.
pub fn daemonHealthy(state: *DaemonState, now_ms: i64) bool {
    if (!state.running.load(.acquire)) return false;
    if (state.consecutive_failures.load(.acquire) > 0) return false;

    const last_success_ms = state.last_success_ms.load(.acquire);
    if (last_success_ms <= 0) return false;

    const max_staleness_ms_u64 = state.interval_seconds * 3 * std.time.ms_per_s;
    const max_staleness_ms: i64 = @intCast(@min(max_staleness_ms_u64, @as(u64, std.math.maxInt(i64))));
    return now_ms - last_success_ms <= max_staleness_ms;
}
