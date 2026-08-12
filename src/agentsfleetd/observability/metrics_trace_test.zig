const std = @import("std");
const trace_policy = @import("../http/route_trace.zig");
const metrics_trace = @import("metrics_trace.zig");
const window = @import("otel_metrics_window_test.zig");

test "the wire carries every suppression reason under its pinned literal label" {
    // The reason labels derive from the admission policy's enum tag names, so
    // an enum-member rename would silently rename dashboard-facing series.
    // These literals are the freeze — they must never track the enum.
    const PINNED_REASONS = [_][]const u8{
        "noisy_route", // pin test: literal is the contract
        "runner_rejection_budget", // pin test: literal is the contract
        "server_error_budget", // pin test: literal is the contract
        "sampled_success_budget", // pin test: literal is the contract
        "sample_miss", // pin test: literal is the contract
    };
    const alloc = std.testing.allocator;
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    for (PINNED_REASONS) |reason| {
        var frag_buf: [96]u8 = undefined;
        const reason_attr = try window.attrFragment(&frag_buf, "reason", reason);
        try window.expectFamilyWith(body, metrics_trace.SUPPRESSED_NAME, &.{reason_attr});
    }
}

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
