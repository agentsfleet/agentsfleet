//! Zombie execution counters + wall-time histogram.
//! Re-exported by metrics_counters.zig; lives in its own module to keep
//! metrics_counters.zig under the 350-line gate (RULE FLL).

const std = @import("std");

// Buckets are stored in milliseconds; rendered as fractional seconds in
// metrics_render (Prometheus base unit is seconds). Sub-second buckets capture
// fast deliveries — most zombies complete well under 1s.
pub const ZombieDurationBucketsMs = [_]u64{ 100, 500, 1_000, 5_000, 10_000, 30_000, 60_000, 300_000, 600_000 };

pub const ZombieHistogramSnapshot = struct {
    buckets: [ZombieDurationBucketsMs.len]u64 = [_]u64{0} ** ZombieDurationBucketsMs.len,
    count: u64 = 0,
    sum: u64 = 0,
};

pub const ZombieFields = struct {
    zombie_triggered_total: u64,
    zombie_completed_total: u64,
    zombie_failed_total: u64,
    zombie_tokens_total: u64,
    zombie_execution_seconds: ZombieHistogramSnapshot,
};

var g_triggered = std.atomic.Value(u64).init(0);
var g_completed = std.atomic.Value(u64).init(0);
var g_failed = std.atomic.Value(u64).init(0);
var g_tokens = std.atomic.Value(u64).init(0);
var g_hist_count = std.atomic.Value(u64).init(0);
var g_hist_sum = std.atomic.Value(u64).init(0);
var g_hist_buckets: [ZombieDurationBucketsMs.len]std.atomic.Value(u64) = blk: {
    var arr: [ZombieDurationBucketsMs.len]std.atomic.Value(u64) = undefined;
    for (&arr) |*b| b.* = std.atomic.Value(u64).init(0);
    break :blk arr;
};

pub fn incZombiesTriggered() void {
    _ = g_triggered.fetchAdd(1, .monotonic);
}

pub fn incZombiesCompleted() void {
    _ = g_completed.fetchAdd(1, .monotonic);
}

pub fn incZombiesFailed() void {
    _ = g_failed.fetchAdd(1, .monotonic);
}

pub fn addZombieTokens(n: u64) void {
    _ = g_tokens.fetchAdd(n, .monotonic);
}

/// Observe a zombie execution wall-time. Storage is milliseconds so sub-second
/// buckets retain resolution; rendering converts le + sum to fractional seconds.
pub fn observeZombieExecutionSeconds(wall_ms: u64) void {
    for (ZombieDurationBucketsMs, 0..) |bound, i| {
        if (wall_ms <= bound) _ = g_hist_buckets[i].fetchAdd(1, .monotonic);
    }
    _ = g_hist_count.fetchAdd(1, .monotonic);
    _ = g_hist_sum.fetchAdd(wall_ms, .monotonic);
}

pub fn snapshotZombieFields() ZombieFields {
    var h = ZombieHistogramSnapshot{
        .count = g_hist_count.load(.acquire),
        .sum = g_hist_sum.load(.acquire),
    };
    for (&h.buckets, 0..) |*b, i| b.* = g_hist_buckets[i].load(.acquire);
    return .{
        .zombie_triggered_total = g_triggered.load(.acquire),
        .zombie_completed_total = g_completed.load(.acquire),
        .zombie_failed_total = g_failed.load(.acquire),
        .zombie_tokens_total = g_tokens.load(.acquire),
        .zombie_execution_seconds = h,
    };
}

pub fn resetForTest() void {
    g_triggered.store(0, .release);
    g_completed.store(0, .release);
    g_failed.store(0, .release);
    g_tokens.store(0, .release);
    g_hist_count.store(0, .release);
    g_hist_sum.store(0, .release);
    for (&g_hist_buckets) |*b| b.store(0, .release);
}

test "inc and snapshot" {
    resetForTest();
    incZombiesTriggered();
    incZombiesTriggered();
    incZombiesCompleted();
    incZombiesFailed();
    addZombieTokens(1500);
    observeZombieExecutionSeconds(4_200);
    const s = snapshotZombieFields();
    try std.testing.expectEqual(@as(u64, 2), s.zombie_triggered_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombie_completed_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombie_failed_total);
    try std.testing.expectEqual(@as(u64, 1500), s.zombie_tokens_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 4_200), s.zombie_execution_seconds.sum);
    resetForTest();
}
