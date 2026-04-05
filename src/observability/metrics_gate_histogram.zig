//! M28_001 §4.1: Gate repair loops per run histogram.
//! Separated from metrics_counters.zig for the 500-line limit.

const std = @import("std");

pub const GateLoopBuckets = [_]u64{ 0, 1, 2, 3, 5, 10 };

pub const GateLoopHistogramSnapshot = struct {
    buckets: [GateLoopBuckets.len]u64 = [_]u64{0} ** GateLoopBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

var g_gate_repair_loops_per_run = GateLoopHistogramSnapshot{};

/// Record gate repair loop count for a run. Thread-safe via external mutex.
pub fn observe(mu: *std.Thread.Mutex, loops: u32) void {
    mu.lock();
    defer mu.unlock();
    const v: u64 = @intCast(loops);
    g_gate_repair_loops_per_run.count += 1;
    g_gate_repair_loops_per_run.sum += v;
    for (GateLoopBuckets, 0..) |le, i| {
        if (v <= le) g_gate_repair_loops_per_run.buckets[i] += 1;
    }
}

pub fn snapshot() GateLoopHistogramSnapshot {
    return g_gate_repair_loops_per_run;
}
