//! Zombie execution counters + wall-time histogram.
//! Re-exported by metrics_counters.zig; lives in its own module to keep
//! metrics_counters.zig under the 350-line gate (RULE FLL).

const std = @import("std");

pub const ZombieDurationBuckets = [_]u64{ 1, 5, 10, 30, 60, 120, 300, 600 };

pub const ZombieHistogramSnapshot = struct {
    buckets: [ZombieDurationBuckets.len]u64 = [_]u64{0} ** ZombieDurationBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

pub const ZombieFields = struct {
    zombies_triggered_total: u64,
    zombies_completed_total: u64,
    zombies_failed_total: u64,
    zombie_tokens_total: u64,
    zombie_execution_seconds: ZombieHistogramSnapshot,
};

var g_triggered = std.atomic.Value(u64).init(0);
var g_completed = std.atomic.Value(u64).init(0);
var g_failed = std.atomic.Value(u64).init(0);
var g_tokens = std.atomic.Value(u64).init(0);
var g_hist_count = std.atomic.Value(u64).init(0);
var g_hist_sum = std.atomic.Value(u64).init(0);
var g_hist_buckets: [ZombieDurationBuckets.len]std.atomic.Value(u64) = blk: {
    var arr: [ZombieDurationBuckets.len]std.atomic.Value(u64) = undefined;
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

pub fn observeZombieExecutionSeconds(wall_ms: u64) void {
    const secs = wall_ms / 1000;
    for (ZombieDurationBuckets, 0..) |bound, i| {
        if (secs <= bound) _ = g_hist_buckets[i].fetchAdd(1, .monotonic);
    }
    _ = g_hist_count.fetchAdd(1, .monotonic);
    _ = g_hist_sum.fetchAdd(secs, .monotonic);
}

pub fn snapshotZombieFields() ZombieFields {
    var h = ZombieHistogramSnapshot{
        .count = g_hist_count.load(.acquire),
        .sum = g_hist_sum.load(.acquire),
    };
    for (&h.buckets, 0..) |*b, i| b.* = g_hist_buckets[i].load(.acquire);
    return .{
        .zombies_triggered_total = g_triggered.load(.acquire),
        .zombies_completed_total = g_completed.load(.acquire),
        .zombies_failed_total = g_failed.load(.acquire),
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
    try std.testing.expectEqual(@as(u64, 2), s.zombies_triggered_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombies_completed_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombies_failed_total);
    try std.testing.expectEqual(@as(u64, 1500), s.zombie_tokens_total);
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 4), s.zombie_execution_seconds.sum);
    resetForTest();
}
