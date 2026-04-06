//! M21_002 §4.3: Interrupt delivery latency histogram.
//! Tracks time from gate command start to interrupt detection (ms).
//! Separated from metrics_counters.zig for the 500-line limit.

const std = @import("std");

pub const InterruptLatencyBuckets = [_]u64{ 500, 1000, 2000, 3000, 5000, 10000, 30000 };

pub const InterruptLatencySnapshot = struct {
    buckets: [InterruptLatencyBuckets.len]u64 = [_]u64{0} ** InterruptLatencyBuckets.len,
    count: u64 = 0,
    sum: u64 = 0,
};

var g_interrupt_delivery_latency_ms = InterruptLatencySnapshot{};

/// Record interrupt delivery latency. Thread-safe via external mutex.
pub fn observe(mu: *std.Thread.Mutex, ms: u64) void {
    mu.lock();
    defer mu.unlock();
    g_interrupt_delivery_latency_ms.count += 1;
    g_interrupt_delivery_latency_ms.sum += ms;
    for (InterruptLatencyBuckets, 0..) |le, i| {
        if (ms <= le) g_interrupt_delivery_latency_ms.buckets[i] += 1;
    }
}

pub fn snapshot() InterruptLatencySnapshot {
    return g_interrupt_delivery_latency_ms;
}
