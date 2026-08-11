//! Aggregate process-memory and plaintext-erasure metrics.
//!
//! The counters are unlabeled and allocator-free: no tenant, workspace, fleet,
//! route, individual secret size, or token material enters telemetry. Storage
//! lives in the generated instrument layer (otel_instruments.zig); the
//! resident-set gauge stays a flush-time live read (otel_metrics_runtime.zig)
//! so an unreportable platform yields absence, never a fake zero.

const instruments = @import("otel_instruments.zig");

pub const METRIC_PROCESS_RESIDENT_MEMORY = "agentsfleet_process_resident_memory_bytes";
pub const METRIC_REQUEST_ERASED_BYTES = "agentsfleet_sensitive_request_erased_bytes_total";
pub const METRIC_RESPONSE_ERASED_BYTES = "agentsfleet_sensitive_response_erased_bytes_total";
pub const METRIC_RESPONSE_WRITE_FAILURES = "agentsfleet_sensitive_response_write_failures_total";

pub const Snapshot = struct {
    request_erased_bytes_total: u64,
    response_erased_bytes_total: u64,
    response_write_failures_total: u64,
};

pub fn recordRequestErased(bytes: usize) void {
    if (bytes == 0) return;
    instruments.add(.sensitive_request_erased_bytes, .{}, @intCast(bytes));
}

pub fn recordResponseErased(bytes: usize) void {
    if (bytes == 0) return;
    instruments.add(.sensitive_response_erased_bytes, .{}, @intCast(bytes));
}

pub fn incResponseWriteFailure() void {
    instruments.inc(.sensitive_response_write_failures, .{});
}

pub fn snapshot() Snapshot {
    return .{
        .request_erased_bytes_total = instruments.snapshotCell(.sensitive_request_erased_bytes, .{}),
        .response_erased_bytes_total = instruments.snapshotCell(.sensitive_response_erased_bytes, .{}),
        .response_write_failures_total = instruments.snapshotCell(.sensitive_response_write_failures, .{}),
    };
}
