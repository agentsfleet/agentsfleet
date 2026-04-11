//! Histogram observation functions — extracted from metrics_counters.zig
//! for the 500-line limit (M21_002).

const std = @import("std");
const mc = @import("metrics_counters.zig");
const gate_hist = @import("metrics_gate_histogram.zig");
const interrupt_hist = @import("metrics_interrupt_histogram.zig");

var g_histograms_mu: std.Thread.Mutex = .{};
var g_agent_duration_seconds = mc.HistogramSnapshot{};

fn observeHistogram(hist: *mc.HistogramSnapshot, value: u64) void {
    hist.count += 1;
    hist.sum += value;
    for (mc.DurationBuckets, 0..) |le, i| {
        if (value <= le) hist.buckets[i] += 1;
    }
}

pub fn observeAgentDurationSeconds(seconds: u64) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    observeHistogram(&g_agent_duration_seconds, seconds);
}

/// M28_001 §4.2: Record gate repair loop count for a run.
pub fn observeGateRepairLoopsPerRun(loops: u32) void {
    gate_hist.observe(&g_histograms_mu, loops);
}

/// M21_002 §4.3: Record interrupt delivery latency in milliseconds.
pub fn observeInterruptDeliveryLatencyMs(ms: u64) void {
    interrupt_hist.observe(&g_histograms_mu, ms);
}

/// Snapshot all histograms under the shared mutex.
pub fn snapshotHistograms(s: *mc.Snapshot) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    s.agent_duration_seconds = g_agent_duration_seconds;
    s.gate_repair_loops_per_run = gate_hist.snapshot();
    s.interrupt_delivery_latency_ms = interrupt_hist.snapshot();
}
