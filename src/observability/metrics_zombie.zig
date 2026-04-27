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

const ZombieFields = struct {
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

// Spec §6.0 row 1.1 — comptime field presence on the exported Snapshot struct.
test "1.1: snapshot_has_zombie_fields" {
    const ZF = @TypeOf(snapshotZombieFields());
    // comptime field access — compile fails if any field missing
    _ = @as(u64, @field(ZF{
        .zombie_triggered_total = 0,
        .zombie_completed_total = 0,
        .zombie_failed_total = 0,
        .zombie_tokens_total = 0,
        .zombie_execution_seconds = .{},
    }, "zombie_triggered_total"));
}

// Spec §6.0 row 1.2 — single call produces exactly +1 delta on the triggered counter.
test "1.2: inc_triggered_increments_counter" {
    resetForTest();
    defer resetForTest();
    const before = snapshotZombieFields().zombie_triggered_total;
    incZombiesTriggered();
    const after = snapshotZombieFields().zombie_triggered_total;
    try std.testing.expectEqual(@as(u64, 1), after - before);
}

// Spec §6.0 row 1.3 — addZombieTokens accumulates monotonically.
test "1.3: add_tokens_accumulates" {
    resetForTest();
    defer resetForTest();
    addZombieTokens(1500);
    addZombieTokens(500);
    try std.testing.expectEqual(@as(u64, 2000), snapshotZombieFields().zombie_tokens_total);
}

// T2 — histogram bucket boundaries. wall_ms = exact bucket value lands in that bucket.
test "T2: observe at exact bucket boundary increments that bucket and all larger ones" {
    resetForTest();
    defer resetForTest();
    observeZombieExecutionSeconds(1_000); // exactly the 1s bucket
    const s = snapshotZombieFields();
    // Cumulative histogram: every bucket with bound >= 1000ms sees this observation.
    for (ZombieDurationBucketsMs, 0..) |bound, i| {
        const expected: u64 = if (bound >= 1_000) 1 else 0;
        try std.testing.expectEqual(expected, s.zombie_execution_seconds.buckets[i]);
    }
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 1_000), s.zombie_execution_seconds.sum);
}

// T2 — wall_ms beyond the largest bucket contributes to count + sum but no named bucket.
test "T2: observe above largest bucket only hits +Inf" {
    resetForTest();
    defer resetForTest();
    observeZombieExecutionSeconds(900_000); // 15 min, above 600_000 (10m) largest bucket
    const s = snapshotZombieFields();
    for (s.zombie_execution_seconds.buckets) |b| {
        try std.testing.expectEqual(@as(u64, 0), b);
    }
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 900_000), s.zombie_execution_seconds.sum);
}

// T2 — wall_ms = 0 (sub-millisecond event) goes into every bucket.
test "T2: observe zero lands in every bucket" {
    resetForTest();
    defer resetForTest();
    observeZombieExecutionSeconds(0);
    const s = snapshotZombieFields();
    for (s.zombie_execution_seconds.buckets) |b| {
        try std.testing.expectEqual(@as(u64, 1), b);
    }
}

// T1 — incZombiesCompleted produces exactly +1 delta.
test "T1: inc_completed_increments_counter" {
    resetForTest();
    defer resetForTest();
    incZombiesCompleted();
    try std.testing.expectEqual(@as(u64, 1), snapshotZombieFields().zombie_completed_total);
}

// T1 — incZombiesFailed produces exactly +1 delta.
test "T1: inc_failed_increments_counter" {
    resetForTest();
    defer resetForTest();
    incZombiesFailed();
    try std.testing.expectEqual(@as(u64, 1), snapshotZombieFields().zombie_failed_total);
}

// T2 — addZombieTokens(0) is a no-op but must not corrupt state.
test "T2: add_zero_tokens_is_identity" {
    resetForTest();
    defer resetForTest();
    addZombieTokens(1000);
    addZombieTokens(0);
    try std.testing.expectEqual(@as(u64, 1000), snapshotZombieFields().zombie_tokens_total);
}

// T2 — observation at smallest bucket (100ms) fills every bucket.
test "T2: observe at smallest bucket boundary fills all buckets" {
    resetForTest();
    defer resetForTest();
    observeZombieExecutionSeconds(100);
    const s = snapshotZombieFields();
    for (s.zombie_execution_seconds.buckets) |b| {
        try std.testing.expectEqual(@as(u64, 1), b);
    }
    try std.testing.expectEqual(@as(u64, 1), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, 100), s.zombie_execution_seconds.sum);
}

// T5 — concurrent increments never lose updates (atomic.fetchAdd contract).
test "T5: concurrent incZombiesTriggered from N threads" {
    resetForTest();
    defer resetForTest();
    const Runner = struct {
        fn run(iters: usize) void {
            var i: usize = 0;
            while (i < iters) : (i += 1) incZombiesTriggered();
        }
    };
    const thread_count = 4;
    const per_thread = 1_000;
    var threads: [thread_count]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Runner.run, .{per_thread});
    for (threads) |t| t.join();
    try std.testing.expectEqual(
        @as(u64, thread_count * per_thread),
        snapshotZombieFields().zombie_triggered_total,
    );
}

// T5 — concurrent observeZombieExecutionSeconds preserves count + sum.
test "T5: concurrent observe does not lose observations" {
    resetForTest();
    defer resetForTest();
    const Runner = struct {
        fn run(iters: usize) void {
            var i: usize = 0;
            while (i < iters) : (i += 1) observeZombieExecutionSeconds(1_000);
        }
    };
    const thread_count = 4;
    const per_thread = 500;
    var threads: [thread_count]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, Runner.run, .{per_thread});
    for (threads) |t| t.join();
    const s = snapshotZombieFields();
    try std.testing.expectEqual(@as(u64, thread_count * per_thread), s.zombie_execution_seconds.count);
    try std.testing.expectEqual(@as(u64, thread_count * per_thread * 1_000), s.zombie_execution_seconds.sum);
}
