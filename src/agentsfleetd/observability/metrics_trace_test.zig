const std = @import("std");
const trace_policy = @import("../http/route_trace.zig");
const metrics_trace = @import("metrics_trace.zig");

test "suppression counters retain fixed reason cardinality" {
    metrics_trace.resetForTest();
    defer metrics_trace.resetForTest();

    metrics_trace.inc(.noisy_route);
    metrics_trace.inc(.server_error_budget);
    metrics_trace.inc(.sample_miss);
    const s = metrics_trace.snapshot();
    try std.testing.expectEqual(@as(u64, 1), s.noisy_route_total);
    try std.testing.expectEqual(@as(u64, 1), s.server_error_budget_total);
    try std.testing.expectEqual(@as(u64, 1), s.sample_miss_total);
}

test "policy reasons can be recorded without allocating" {
    metrics_trace.resetForTest();
    defer metrics_trace.resetForTest();
    metrics_trace.inc(switch (trace_policy.Decision{ .suppress = .runner_rejection_budget }) {
        .suppress => |reason| reason,
        .emit => unreachable,
    });
    try std.testing.expectEqual(@as(u64, 1), metrics_trace.snapshot().runner_rejection_budget_total);
}
