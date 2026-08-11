//! Aggregate process-memory and plaintext-erasure metrics.
//!
//! The counters are unlabeled and allocator-free: no tenant, workspace, fleet,
//! route, individual secret size, or token material enters telemetry.

const std = @import("std");

pub const METRIC_PROCESS_RESIDENT_MEMORY = "agentsfleet_process_resident_memory_bytes";
pub const METRIC_REQUEST_ERASED_BYTES = "agentsfleet_sensitive_request_erased_bytes_total";
pub const METRIC_RESPONSE_ERASED_BYTES = "agentsfleet_sensitive_response_erased_bytes_total";
pub const METRIC_RESPONSE_WRITE_FAILURES = "agentsfleet_sensitive_response_write_failures_total";

pub const Snapshot = struct {
    request_erased_bytes_total: u64,
    response_erased_bytes_total: u64,
    response_write_failures_total: u64,
};

var g_request_erased_bytes_total = std.atomic.Value(u64).init(0);
var g_response_erased_bytes_total = std.atomic.Value(u64).init(0);
var g_response_write_failures_total = std.atomic.Value(u64).init(0);

pub fn recordRequestErased(bytes: usize) void {
    if (bytes == 0) return;
    _ = g_request_erased_bytes_total.fetchAdd(@intCast(bytes), .monotonic);
}

pub fn recordResponseErased(bytes: usize) void {
    if (bytes == 0) return;
    _ = g_response_erased_bytes_total.fetchAdd(@intCast(bytes), .monotonic);
}

pub fn incResponseWriteFailure() void {
    _ = g_response_write_failures_total.fetchAdd(1, .monotonic);
}

pub fn snapshot() Snapshot {
    return .{
        .request_erased_bytes_total = g_request_erased_bytes_total.load(.acquire),
        .response_erased_bytes_total = g_response_erased_bytes_total.load(.acquire),
        .response_write_failures_total = g_response_write_failures_total.load(.acquire),
    };
}
