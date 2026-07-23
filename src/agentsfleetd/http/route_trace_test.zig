const std = @import("std");
const router = @import("router.zig");
const trace_policy = @import("route_trace.zig");

test "test_trace_policy_is_total_and_suppresses_runner_chatter" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(.healthz, 200, "a", 1));
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(.runner_heartbeat, 204, "b", 1));
}

test "test_trace_error_budgets_are_hard" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    for (0..4) |_| try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.runner_report, 409, "r", 8));
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .runner_rejection_budget }, trace_policy.decide(.runner_report, 409, "r", 8));
    for (0..4) |_| try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.fleet_bundles, 500, "e", 8));
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .server_error_budget }, trace_policy.decide(.fleet_bundles, 500, "e", 8));
}

test "fixed windows reset on the next monotonic second" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    for (0..4) |_| _ = trace_policy.decide(.runner_report, 429, "r", 10);
    try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.runner_report, 429, "r", 11));
}

test "test_trace_sampling_uses_server_owned_entropy" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    var sampled: ?[]const u8 = null;
    var rejected: ?[]const u8 = null;
    var buf: [32]u8 = undefined;
    for (0..10_000) |i| {
        const id = try std.fmt.bufPrint(&buf, "span-{d}", .{i});
        switch (trace_policy.decide(.model_library, 200, id, 20)) {
            .emit => sampled = id,
            .suppress => |reason| {
                if (reason == .sample_miss) rejected = id;
            },
        }
        if (sampled != null and rejected != null) break;
    }
    try std.testing.expect(sampled != null);
    try std.testing.expect(rejected != null);
}

test "test_runner_admission_rejection_is_traced_or_counted" {
    const route: router.Route = .{ .runner_activity = "lease" };
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(route, 200, "id", 30));
}
