//! Aggregate process-memory and plaintext-erasure metrics.
//!
//! The counters are unlabeled and allocator-free: no tenant, workspace, fleet,
//! route, individual secret size, or token material enters telemetry.

const std = @import("std");
const common = @import("common");

pub const METRIC_PROCESS_RESIDENT_MEMORY = "fleet_process_resident_memory_bytes";
pub const METRIC_REQUEST_ERASED_BYTES = "fleet_sensitive_request_erased_bytes_total";
pub const METRIC_RESPONSE_ERASED_BYTES = "fleet_sensitive_response_erased_bytes_total";
pub const METRIC_RESPONSE_WRITE_FAILURES = "fleet_sensitive_response_write_failures_total";

const TYPE_GAUGE = "gauge";
const TYPE_COUNTER = "counter";
const HELP_PROCESS_RESIDENT_MEMORY = "Current agentsfleetd resident set size in bytes.";
const HELP_REQUEST_ERASED_BYTES = "Secret-bearing HTTP request body bytes erased after dispatch.";
const HELP_RESPONSE_ERASED_BYTES = "Serialized secret-bearing HTTP response bytes erased after write.";
const HELP_RESPONSE_WRITE_FAILURES = "Sensitive HTTP response writes that failed and forced connection close.";

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

pub fn renderPrometheus(writer: anytype) !void {
    const s = snapshot();
    if (common.rss.currentBytes()) |resident_bytes| {
        try appendMetric(writer, METRIC_PROCESS_RESIDENT_MEMORY, TYPE_GAUGE, HELP_PROCESS_RESIDENT_MEMORY, resident_bytes);
    }
    try appendMetric(writer, METRIC_REQUEST_ERASED_BYTES, TYPE_COUNTER, HELP_REQUEST_ERASED_BYTES, s.request_erased_bytes_total);
    try appendMetric(writer, METRIC_RESPONSE_ERASED_BYTES, TYPE_COUNTER, HELP_RESPONSE_ERASED_BYTES, s.response_erased_bytes_total);
    try appendMetric(writer, METRIC_RESPONSE_WRITE_FAILURES, TYPE_COUNTER, HELP_RESPONSE_WRITE_FAILURES, s.response_write_failures_total);
}

fn appendMetric(writer: anytype, name: []const u8, metric_type: []const u8, help: []const u8, value: u64) !void {
    try writer.print("# HELP {s} {s}\n", .{ name, help });
    try writer.print("# TYPE {s} {s}\n", .{ name, metric_type });
    try writer.print("{s} {d}\n", .{ name, value });
}
