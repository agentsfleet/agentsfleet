//! Executor-specific metrics counters.
//!
//! These are separate from the main metrics_counters.zig so the executor
//! sidecar binary does not need to link the full observability stack.
//! The zombied binary re-exports these through the main metrics facade.

const std = @import("std");

var g_executor_sessions_created_total = std.atomic.Value(u64).init(0);
var g_executor_sessions_active = std.atomic.Value(u64).init(0);
var g_executor_failures_total = std.atomic.Value(u64).init(0);
var g_executor_oom_kills_total = std.atomic.Value(u64).init(0);
var g_executor_timeout_kills_total = std.atomic.Value(u64).init(0);
var g_executor_landlock_denials_total = std.atomic.Value(u64).init(0);
var g_executor_resource_kills_total = std.atomic.Value(u64).init(0);
var g_executor_lease_expired_total = std.atomic.Value(u64).init(0);
var g_executor_cancellations_total = std.atomic.Value(u64).init(0);
var g_executor_cpu_throttled_ms_total = std.atomic.Value(u64).init(0);
var g_executor_memory_peak_bytes = std.atomic.Value(u64).init(0);

pub fn incExecutorSessionsCreated() void {
    _ = g_executor_sessions_created_total.fetchAdd(1, .monotonic);
}

pub fn setExecutorSessionsActive(v: u32) void {
    g_executor_sessions_active.store(@as(u64, @intCast(v)), .release);
}

pub fn incExecutorFailures() void {
    _ = g_executor_failures_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorOomKills() void {
    _ = g_executor_oom_kills_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorTimeoutKills() void {
    _ = g_executor_timeout_kills_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorLandlockDenials() void {
    _ = g_executor_landlock_denials_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorResourceKills() void {
    _ = g_executor_resource_kills_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorLeaseExpired() void {
    _ = g_executor_lease_expired_total.fetchAdd(1, .monotonic);
}

pub fn incExecutorCancellations() void {
    _ = g_executor_cancellations_total.fetchAdd(1, .monotonic);
}

pub fn addExecutorCpuThrottledMs(ms: u64) void {
    _ = g_executor_cpu_throttled_ms_total.fetchAdd(ms, .monotonic);
}

pub fn setExecutorMemoryPeakBytes(bytes: u64) void {
    const current = g_executor_memory_peak_bytes.load(.acquire);
    if (bytes > current) {
        g_executor_memory_peak_bytes.store(bytes, .release);
    }
}

pub const ExecutorSnapshot = struct {
    sessions_created_total: u64,
    sessions_active: u64,
    failures_total: u64,
    oom_kills_total: u64,
    timeout_kills_total: u64,
    landlock_denials_total: u64,
    resource_kills_total: u64,
    lease_expired_total: u64,
    cancellations_total: u64,
    cpu_throttled_ms_total: u64,
    memory_peak_bytes: u64,
};

pub fn executorSnapshot() ExecutorSnapshot {
    return .{
        .sessions_created_total = g_executor_sessions_created_total.load(.acquire),
        .sessions_active = g_executor_sessions_active.load(.acquire),
        .failures_total = g_executor_failures_total.load(.acquire),
        .oom_kills_total = g_executor_oom_kills_total.load(.acquire),
        .timeout_kills_total = g_executor_timeout_kills_total.load(.acquire),
        .landlock_denials_total = g_executor_landlock_denials_total.load(.acquire),
        .resource_kills_total = g_executor_resource_kills_total.load(.acquire),
        .lease_expired_total = g_executor_lease_expired_total.load(.acquire),
        .cancellations_total = g_executor_cancellations_total.load(.acquire),
        .cpu_throttled_ms_total = g_executor_cpu_throttled_ms_total.load(.acquire),
        .memory_peak_bytes = g_executor_memory_peak_bytes.load(.acquire),
    };
}

test "executor metrics increment correctly" {
    incExecutorSessionsCreated();
    incExecutorFailures();
    incExecutorOomKills();
    incExecutorTimeoutKills();
    incExecutorLandlockDenials();
    incExecutorResourceKills();
    incExecutorLeaseExpired();
    incExecutorCancellations();
    setExecutorSessionsActive(3);
    setExecutorMemoryPeakBytes(1024 * 1024);
    addExecutorCpuThrottledMs(500);

    const snap = executorSnapshot();
    try std.testing.expect(snap.sessions_created_total >= 1);
    try std.testing.expect(snap.failures_total >= 1);
    try std.testing.expect(snap.oom_kills_total >= 1);
    try std.testing.expect(snap.timeout_kills_total >= 1);
    try std.testing.expect(snap.landlock_denials_total >= 1);
    try std.testing.expect(snap.resource_kills_total >= 1);
    try std.testing.expect(snap.lease_expired_total >= 1);
    try std.testing.expect(snap.cancellations_total >= 1);
    try std.testing.expect(snap.sessions_active == 3);
    try std.testing.expect(snap.memory_peak_bytes == 1024 * 1024);
    try std.testing.expect(snap.cpu_throttled_ms_total >= 500);
}
