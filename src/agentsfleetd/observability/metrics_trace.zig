//! Fixed-cardinality counters for trace admission decisions. Storage lives in
//! the generated instrument layer (otel_instruments.zig); the registry
//! declares the `reason` dimension off the admission policy's own enum.

const trace_policy = @import("../http/route_trace.zig");
const instruments = @import("otel_instruments.zig");

pub const SUPPRESSED_NAME = "agentsfleet_http_trace_suppressed_total";

/// The label source for the suppression `reason` dimension: the registry
/// (otel_metrics_families.zig) declares the dimension with this enum, so its
/// tag names are the wire values.
pub const SuppressionReason = trace_policy.SuppressionReason;

pub const Snapshot = struct {
    noisy_route_total: u64,
    runner_rejection_budget_total: u64,
    server_error_budget_total: u64,
    sampled_success_budget_total: u64,
    sample_miss_total: u64,
};

pub fn inc(reason: SuppressionReason) void {
    instruments.inc(.http_trace_suppressed, .{ .reason = reason });
}

pub fn snapshot() Snapshot {
    return .{
        .noisy_route_total = instruments.snapshotCell(.http_trace_suppressed, .{ .reason = .noisy_route }),
        .runner_rejection_budget_total = instruments.snapshotCell(.http_trace_suppressed, .{ .reason = .runner_rejection_budget }),
        .server_error_budget_total = instruments.snapshotCell(.http_trace_suppressed, .{ .reason = .server_error_budget }),
        .sampled_success_budget_total = instruments.snapshotCell(.http_trace_suppressed, .{ .reason = .sampled_success_budget }),
        .sample_miss_total = instruments.snapshotCell(.http_trace_suppressed, .{ .reason = .sample_miss }),
    };
}

pub fn resetForTest() void {
    instruments.resetCellsForTest(&.{.http_trace_suppressed});
}

test {
    _ = @import("metrics_trace_test.zig");
}
