//! The benchmark's single output surface. Every §-mandated metric is a struct
//! field, so the JSON emit carries the full set by construction — a report
//! that omits variance, cost, or the threshold-win cases cannot typecheck.

const std = @import("std");
const manifest = @import("manifest.zig");

/// A seed the frozen thresholds alone would have caught. The sweep is honest,
/// not embarrassed, about these — obvious spikes are expected threshold wins.
pub const ThresholdWin = struct {
    /// Borrowed from the manifest that produced the score; freed with it.
    seed_id: []const u8,
    class: manifest.IncidentClass,
    reason: []const u8,
};

pub const Report = struct {
    total_incidents: u32,
    detected_incidents: u32,
    recall_pct: u32,
    false_positives: u32,
    ttd_median_ms: i64,
    ttd_p95_ms: i64,
    ttar_median_ms: i64,
    /// Mean per-incident time-to-detect variance across repeated runs; zero
    /// for a single run.
    ttd_variance_ms: i64,
    query_cost_usd_micros: u64,
    model_cost_usd_micros: u64,
    threshold_wins: []const ThresholdWin,

    /// Frees the slice score() allocated; seed ids inside stay borrowed from
    /// the manifest.
    pub fn deinit(self: *Report, alloc: std.mem.Allocator) void {
        alloc.free(self.threshold_wins);
        self.* = undefined;
    }
};

/// Caller must free the returned bytes.
pub fn emitJson(alloc: std.mem.Allocator, r: Report) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, r, .{ .whitespace = .indent_2 });
}
