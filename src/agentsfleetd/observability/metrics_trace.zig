//! Fixed-cardinality counters for trace admission decisions.

const std = @import("std");
const trace_policy = @import("../http/route_trace.zig");

pub const SUPPRESSED_NAME = "agentsfleet_http_trace_suppressed_total";
pub const SUPPRESSED_HELP = "HTTP request spans suppressed by the bounded trace admission policy.";

pub const Snapshot = struct {
    noisy_route_total: u64,
    runner_rejection_budget_total: u64,
    server_error_budget_total: u64,
    sampled_success_budget_total: u64,
    sample_miss_total: u64,
};

var g_noisy_route = std.atomic.Value(u64).init(0);
var g_runner_rejection_budget = std.atomic.Value(u64).init(0);
var g_server_error_budget = std.atomic.Value(u64).init(0);
var g_sampled_success_budget = std.atomic.Value(u64).init(0);
var g_sample_miss = std.atomic.Value(u64).init(0);

pub fn inc(reason: trace_policy.SuppressionReason) void {
    const counter = switch (reason) {
        .noisy_route => &g_noisy_route,
        .runner_rejection_budget => &g_runner_rejection_budget,
        .server_error_budget => &g_server_error_budget,
        .sampled_success_budget => &g_sampled_success_budget,
        .sample_miss => &g_sample_miss,
    };
    // safe because: these are independent observability counters; scrape-time
    // staleness is acceptable and no data is published through them.
    _ = counter.fetchAdd(1, .monotonic);
}

pub fn snapshot() Snapshot {
    return .{
        .noisy_route_total = g_noisy_route.load(.acquire),
        .runner_rejection_budget_total = g_runner_rejection_budget.load(.acquire),
        .server_error_budget_total = g_server_error_budget.load(.acquire),
        .sampled_success_budget_total = g_sampled_success_budget.load(.acquire),
        .sample_miss_total = g_sample_miss.load(.acquire),
    };
}

pub fn resetForTest() void {
    g_noisy_route.store(0, .release);
    g_runner_rejection_budget.store(0, .release);
    g_server_error_budget.store(0, .release);
    g_sampled_success_budget.store(0, .release);
    g_sample_miss.store(0, .release);
}

test {
    _ = @import("metrics_trace_test.zig");
}
