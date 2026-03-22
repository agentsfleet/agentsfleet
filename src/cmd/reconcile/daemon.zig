//! Daemon lifecycle management for the reconcile process.
//!
//! Exports:
//!   - `DaemonState`               — re-exported from state.zig for convenience.
//!   - `daemon_shutdown_requested` — global signal flag set by the OS signal handler.
//!   - `g_daemon_state`            — re-exported from state.zig; global pointer to active state.
//!   - `daemonHealthy`             — staleness + failure gate used by /healthz.
//!   - `tryAcquireLeaderLock`      — advisory PG lock for leader election.
//!   - `releaseLeaderLock`         — release the advisory lock.
//!   - `onSignal`                  — POSIX signal handler (stores true atomically).
//!   - `installSignalHandlers`     — register SIGINT + SIGTERM handlers.
//!   - `runDaemon`                 — main daemon loop.
//!
//! Ownership:
//!   - `DaemonState` is stack-allocated inside `runDaemon`; `g_daemon_state`
//!     is set to point at it and cleared on exit via defer.

const std = @import("std");
const zap = @import("zap");
const db = @import("../../db/pool.zig");
const posthog = @import("posthog");
const tick_mod = @import("tick.zig");
const metrics_mod = @import("metrics.zig");
const state_mod = @import("state.zig");

pub const DaemonState = state_mod.DaemonState;
pub const daemonHealthy = state_mod.daemonHealthy;

const log = std.log.scoped(.reconcile);
const ReconcileLeaderLockKey: i64 = 0x7A6F6D6269651001;

pub var daemon_shutdown_requested = std.atomic.Value(bool).init(false);

pub fn tryAcquireLeaderLock(conn: *db.Conn) !bool {
    var q = try conn.query("SELECT pg_try_advisory_lock($1)", .{ReconcileLeaderLockKey});
    defer q.deinit();
    const row = (try q.next()) orelse return false;
    const acquired = try row.get(bool, 0);
    try q.drain();
    return acquired;
}

pub fn releaseLeaderLock(conn: *db.Conn) void {
    var q = conn.query("SELECT pg_advisory_unlock($1)", .{ReconcileLeaderLockKey}) catch return;
    q.drain() catch {};
    q.deinit();
}

pub fn onSignal(sig: i32) callconv(.c) void {
    _ = sig;
    daemon_shutdown_requested.store(true, .release);
}

pub fn installSignalHandlers() void {
    const action = std.posix.Sigaction{
        .handler = .{ .handler = onSignal },
        .mask = std.posix.sigemptyset(),
        .flags = 0,
    };
    std.posix.sigaction(std.posix.SIG.INT, &action, null);
    std.posix.sigaction(std.posix.SIG.TERM, &action, null);
}

pub fn runDaemon(alloc: std.mem.Allocator, pool: *db.Pool, posthog_client: ?*posthog.PostHogClient, interval_seconds: u64, metrics_port: u16) !void {
    daemon_shutdown_requested.store(false, .release);
    installSignalHandlers();

    const lock_conn = pool.acquire() catch |err| {
        std.debug.print("fatal: reconcile leader lock connection acquire failed: {}\n", .{err});
        std.process.exit(1);
    };
    defer pool.release(lock_conn);
    defer releaseLeaderLock(lock_conn);

    const lock_acquired = tryAcquireLeaderLock(lock_conn) catch |err| {
        std.debug.print("fatal: reconcile leader lock failed: {}\n", .{err});
        std.process.exit(1);
    };
    if (!lock_acquired) {
        log.warn("reconcile.lock_held status=skip reason=another process holds lock", .{});
        return;
    }

    var daemon_state = DaemonState{
        .alloc = alloc,
        .interval_seconds = interval_seconds,
        .started_ms = std.time.milliTimestamp(),
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(0),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    state_mod.g_daemon_state = &daemon_state;
    defer state_mod.g_daemon_state = null;

    var metrics_thread = try std.Thread.spawn(.{}, metrics_mod.metricsServerThread, .{metrics_port});
    defer metrics_thread.join();

    log.info("reconcile.daemon_started interval_seconds={d} metrics_port={d}", .{ interval_seconds, metrics_port });
    while (!daemon_shutdown_requested.load(.acquire)) {
        const now_ms = std.time.milliTimestamp();
        daemon_state.last_attempt_ms.store(now_ms, .release);
        _ = daemon_state.total_ticks.fetchAdd(1, .monotonic);

        const ok = tick_mod.runOnce(alloc, pool, posthog_client);
        if (ok) {
            daemon_state.last_success_ms.store(now_ms, .release);
            daemon_state.consecutive_failures.store(0, .release);
        } else {
            _ = daemon_state.consecutive_failures.fetchAdd(1, .monotonic);
        }

        const sleep_ns = interval_seconds * std.time.ns_per_s;
        var elapsed_ns: u64 = 0;
        while (elapsed_ns < sleep_ns and !daemon_shutdown_requested.load(.acquire)) {
            const step_ns = @min(250 * std.time.ns_per_ms, sleep_ns - elapsed_ns);
            std.Thread.sleep(step_ns);
            elapsed_ns += step_ns;
        }
    }

    daemon_state.running.store(false, .release);
    zap.stop();
    log.info("reconcile.daemon_stopped uptime_ms={d} ticks={d}", .{
        std.time.milliTimestamp() - daemon_state.started_ms,
        daemon_state.total_ticks.load(.acquire),
    });
}

// ---------------------------------------------------------------------------
// Tests (moved from reconcile.zig)
// ---------------------------------------------------------------------------

test "daemonHealthy returns false when daemon not running" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 30,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(false),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(10_000),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    try std.testing.expect(!daemonHealthy(&s, 11_000));
}

test "daemonHealthy returns false before first success" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 30,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(0),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    try std.testing.expect(!daemonHealthy(&s, 5_000));
}

test "daemonHealthy returns false on consecutive failures" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 30,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(10_000),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(1),
    };
    try std.testing.expect(!daemonHealthy(&s, 11_000));
}

test "daemonHealthy returns false when success is stale" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 10,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(10_000),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    try std.testing.expect(!daemonHealthy(&s, 50_500));
}

test "daemonHealthy returns true for recent successful tick" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 30,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(10_000),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    try std.testing.expect(daemonHealthy(&s, 15_000));
}

// New T2 test per spec §4.0: daemonHealthy with exactly-at-boundary staleness.
// With interval=10s, max_staleness = 10*3*1000 = 30_000 ms.
// last_success=10_000, now=40_000 → delta=30_000 → exactly at boundary → healthy.
test "daemonHealthy returns true at exactly boundary staleness (interval*3)" {
    var s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 10,
        .started_ms = 1000,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(10_000),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    // delta = 40_000 - 10_000 = 30_000 == max_staleness_ms → still healthy (<=)
    try std.testing.expect(daemonHealthy(&s, 40_000));
    // delta = 40_001 - 10_000 = 30_001 > max_staleness_ms → stale
    try std.testing.expect(!daemonHealthy(&s, 40_001));
}

// T7 regression parity: verify DaemonState field names/types are unchanged.
test "DaemonState field regression parity" {
    const s = DaemonState{
        .alloc = std.testing.allocator,
        .interval_seconds = 60,
        .started_ms = 0,
        .running = std.atomic.Value(bool).init(true),
        .last_attempt_ms = std.atomic.Value(i64).init(0),
        .last_success_ms = std.atomic.Value(i64).init(0),
        .last_dead_lettered = std.atomic.Value(u32).init(0),
        .total_ticks = std.atomic.Value(u64).init(0),
        .consecutive_failures = std.atomic.Value(u32).init(0),
    };
    try std.testing.expectEqual(@as(u64, 60), s.interval_seconds);
    try std.testing.expectEqual(@as(i64, 0), s.started_ms);
    try std.testing.expect(s.running.load(.acquire));
    try std.testing.expectEqual(@as(i64, 0), s.last_attempt_ms.load(.acquire));
    try std.testing.expectEqual(@as(i64, 0), s.last_success_ms.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), s.last_dead_lettered.load(.acquire));
    try std.testing.expectEqual(@as(u64, 0), s.total_ticks.load(.acquire));
    try std.testing.expectEqual(@as(u32, 0), s.consecutive_failures.load(.acquire));
}
