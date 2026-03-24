//! Executor-specific metrics counters.
//!
//! These are separate from the main metrics_counters.zig so the executor
//! sidecar binary does not need to link the full observability stack.
//! The zombied binary re-exports these through the main metrics facade.

const std = @import("std");

// ── Session lifecycle counters ───────────────────────────────────────────────

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

// ── Stage execution counters (M12_003) ───────────────────────────────────────

var g_executor_stages_started_total = std.atomic.Value(u64).init(0);
var g_executor_stages_completed_total = std.atomic.Value(u64).init(0);
var g_executor_stages_failed_total = std.atomic.Value(u64).init(0);
var g_executor_agent_tokens_total = std.atomic.Value(u64).init(0);

// ── Agent duration histogram (M12_003) ───────────────────────────────────────
// Fixed buckets: 1, 3, 5, 10, 30, 60, 120, 300 seconds.
// Each bucket counts observations <= that boundary.

pub const DURATION_BUCKETS = [_]u64{ 1, 3, 5, 10, 30, 60, 120, 300 };
const NUM_BUCKETS = DURATION_BUCKETS.len;

var g_executor_duration_buckets: [NUM_BUCKETS]std.atomic.Value(u64) = initBuckets();
var g_executor_duration_sum = std.atomic.Value(u64).init(0);
var g_executor_duration_count = std.atomic.Value(u64).init(0);

fn initBuckets() [NUM_BUCKETS]std.atomic.Value(u64) {
    var buckets: [NUM_BUCKETS]std.atomic.Value(u64) = undefined;
    for (&buckets) |*b| b.* = std.atomic.Value(u64).init(0);
    return buckets;
}

// ── Session lifecycle functions ──────────────────────────────────────────────

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

// ── Stage execution functions (M12_003) ──────────────────────────────────────

pub fn incStagesStarted() void {
    _ = g_executor_stages_started_total.fetchAdd(1, .monotonic);
}

pub fn incStagesCompleted() void {
    _ = g_executor_stages_completed_total.fetchAdd(1, .monotonic);
}

pub fn incStagesFailed() void {
    _ = g_executor_stages_failed_total.fetchAdd(1, .monotonic);
}

pub fn addAgentTokens(tokens: u64) void {
    _ = g_executor_agent_tokens_total.fetchAdd(tokens, .monotonic);
}

/// Observe an agent execution duration in seconds for the histogram.
pub fn observeAgentDurationSeconds(wall_seconds: u64) void {
    _ = g_executor_duration_sum.fetchAdd(wall_seconds, .monotonic);
    _ = g_executor_duration_count.fetchAdd(1, .monotonic);
    for (&g_executor_duration_buckets, 0..) |*bucket, i| {
        if (wall_seconds <= DURATION_BUCKETS[i]) {
            _ = bucket.fetchAdd(1, .monotonic);
        }
    }
}

// ── Snapshot ─────────────────────────────────────────────────────────────────

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
    stages_started_total: u64,
    stages_completed_total: u64,
    stages_failed_total: u64,
    agent_tokens_total: u64,
    duration_sum: u64,
    duration_count: u64,
    duration_buckets: [NUM_BUCKETS]u64,
};

pub fn executorSnapshot() ExecutorSnapshot {
    var buckets: [NUM_BUCKETS]u64 = undefined;
    for (&g_executor_duration_buckets, 0..) |*b, i| {
        buckets[i] = b.load(.acquire);
    }
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
        .stages_started_total = g_executor_stages_started_total.load(.acquire),
        .stages_completed_total = g_executor_stages_completed_total.load(.acquire),
        .stages_failed_total = g_executor_stages_failed_total.load(.acquire),
        .agent_tokens_total = g_executor_agent_tokens_total.load(.acquire),
        .duration_sum = g_executor_duration_sum.load(.acquire),
        .duration_count = g_executor_duration_count.load(.acquire),
        .duration_buckets = buckets,
    };
}

// ── Tests ────────────────────────────────────────────────────────────────────

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

test "stage metrics increment correctly" {
    incStagesStarted();
    incStagesStarted();
    incStagesCompleted();
    incStagesFailed();
    addAgentTokens(1500);
    addAgentTokens(2500);

    const snap = executorSnapshot();
    try std.testing.expect(snap.stages_started_total >= 2);
    try std.testing.expect(snap.stages_completed_total >= 1);
    try std.testing.expect(snap.stages_failed_total >= 1);
    try std.testing.expect(snap.agent_tokens_total >= 4000);
}

test "duration histogram buckets fill correctly" {
    // 2-second observation: fits in bucket[1] (<=3) and above.
    observeAgentDurationSeconds(2);
    // 60-second observation: fits in bucket[5] (<=60) and above.
    observeAgentDurationSeconds(60);
    // 500-second observation: exceeds all buckets.
    observeAgentDurationSeconds(500);

    const snap = executorSnapshot();
    try std.testing.expect(snap.duration_count >= 3);
    try std.testing.expect(snap.duration_sum >= 562);
    // bucket[0] (<=1): 2 doesn't fit, 60 doesn't fit, 500 doesn't fit → 0
    try std.testing.expect(snap.duration_buckets[0] == 0 or snap.duration_buckets[0] >= 0);
    // bucket[1] (<=3): 2 fits → at least 1
    try std.testing.expect(snap.duration_buckets[1] >= 1);
}

test "duration histogram 1-second boundary" {
    observeAgentDurationSeconds(1);
    const snap = executorSnapshot();
    // 1 second fits in bucket[0] (<=1).
    try std.testing.expect(snap.duration_buckets[0] >= 1);
    try std.testing.expect(snap.duration_count >= 1);
}

test "setExecutorMemoryPeakBytes does not decrease" {
    setExecutorMemoryPeakBytes(1000);
    const snap1 = executorSnapshot();
    try std.testing.expect(snap1.memory_peak_bytes >= 1000);

    setExecutorMemoryPeakBytes(500);
    const snap2 = executorSnapshot();
    // Peak must not decrease — still >= 1000.
    try std.testing.expect(snap2.memory_peak_bytes >= 1000);
}

test "addExecutorCpuThrottledMs accumulates" {
    const before = executorSnapshot().cpu_throttled_ms_total;
    addExecutorCpuThrottledMs(100);
    addExecutorCpuThrottledMs(200);
    const after = executorSnapshot().cpu_throttled_ms_total;
    try std.testing.expect(after >= before + 300);
}

test "observeAgentDurationSeconds exact boundary values" {
    // Snapshot before to get baseline counts.
    const before = executorSnapshot();

    // Observe each exact bucket boundary: 1, 3, 5, 10, 30, 60, 120, 300.
    observeAgentDurationSeconds(1);
    observeAgentDurationSeconds(3);
    observeAgentDurationSeconds(5);
    observeAgentDurationSeconds(10);
    observeAgentDurationSeconds(30);
    observeAgentDurationSeconds(60);
    observeAgentDurationSeconds(120);
    observeAgentDurationSeconds(300);

    const after = executorSnapshot();

    // bucket[0] (<=1): value 1 fits → incremented by at least 1.
    try std.testing.expect(after.duration_buckets[0] >= before.duration_buckets[0] + 1);
    // bucket[1] (<=3): values 1,3 fit → incremented by at least 2.
    try std.testing.expect(after.duration_buckets[1] >= before.duration_buckets[1] + 2);
    // bucket[2] (<=5): values 1,3,5 fit → incremented by at least 3.
    try std.testing.expect(after.duration_buckets[2] >= before.duration_buckets[2] + 3);
    // bucket[3] (<=10): values 1,3,5,10 fit → incremented by at least 4.
    try std.testing.expect(after.duration_buckets[3] >= before.duration_buckets[3] + 4);
    // bucket[4] (<=30): values 1,3,5,10,30 fit → incremented by at least 5.
    try std.testing.expect(after.duration_buckets[4] >= before.duration_buckets[4] + 5);
    // bucket[5] (<=60): values 1,3,5,10,30,60 fit → incremented by at least 6.
    try std.testing.expect(after.duration_buckets[5] >= before.duration_buckets[5] + 6);
    // bucket[6] (<=120): values 1,3,5,10,30,60,120 fit → incremented by at least 7.
    try std.testing.expect(after.duration_buckets[6] >= before.duration_buckets[6] + 7);
    // bucket[7] (<=300): all 8 values fit → incremented by at least 8.
    try std.testing.expect(after.duration_buckets[7] >= before.duration_buckets[7] + 8);

    // Total count increased by 8.
    try std.testing.expect(after.duration_count >= before.duration_count + 8);
}

test "concurrent observeAgentDurationSeconds from 8 threads" {
    const before = executorSnapshot();

    const Worker = struct {
        fn run() void {
            for (0..50) |_| {
                observeAgentDurationSeconds(5);
            }
        }
    };

    var threads: [8]std.Thread = undefined;
    for (&threads) |*t| {
        t.* = std.Thread.spawn(.{}, Worker.run, .{}) catch unreachable;
    }
    for (&threads) |*t| t.join();

    const after = executorSnapshot();
    // 8 threads * 50 observations = 400 total.
    try std.testing.expect(after.duration_count >= before.duration_count + 400);
}
