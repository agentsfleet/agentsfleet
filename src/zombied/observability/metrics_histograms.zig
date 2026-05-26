//! Histogram observation functions — extracted from metrics_counters.zig
//! for the 350-line gate.

const std = @import("std");
const mc = @import("metrics_counters.zig");

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

/// Snapshot all histograms under the shared mutex.
pub fn snapshotHistograms(s: *mc.Snapshot) void {
    g_histograms_mu.lock();
    defer g_histograms_mu.unlock();
    s.agent_duration_seconds = g_agent_duration_seconds;
}
