const std = @import("std");
const router = @import("router.zig");
const trace_policy = @import("route_trace.zig");

const CONCURRENT_CALLERS: usize = 100;

const EmitCounts = struct {
    runner_rejections: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    server_errors: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
    sampled_generic: std.atomic.Value(u32) = std.atomic.Value(u32).init(0),
};

fn countEmit(decision: trace_policy.Decision, counter: *std.atomic.Value(u32)) void {
    switch (decision) {
        .emit => {
            // safe because: the counter is test-only evidence; no state is
            // published through it and the final reads happen after joins.
            _ = counter.fetchAdd(1, .monotonic);
        },
        .suppress => {},
    }
}

fn exerciseConcurrentPolicy(counts: *EmitCounts, sampled_id: []const u8) void {
    countEmit(trace_policy.decide(.runner_report, 409, "runner", 100), &counts.runner_rejections);
    countEmit(trace_policy.decide(.runner_report, 500, "server", 200), &counts.server_errors);
    countEmit(trace_policy.decide(.model_library, 200, sampled_id, 300), &counts.sampled_generic);
}

fn loadCount(counter: *const std.atomic.Value(u32)) u32 {
    // safe because: every writer thread is joined before this acquire load.
    return counter.load(.acquire);
}

fn findSampledId(buf: *[32]u8) ![]const u8 {
    for (0..10_000) |i| {
        trace_policy.resetForTest();
        const id = try std.fmt.bufPrint(buf, "span-{d}", .{i});
        switch (trace_policy.decide(.model_library, 200, id, 1)) {
            .emit => return id,
            .suppress => {},
        }
    }
    return error.SampledIdentifierNotFound;
}

test "test_trace_policy_is_total_and_suppresses_runner_chatter" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(.healthz, 200, "a", 1));
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(.runner_heartbeat, 204, "b", 1));
}

test "test_trace_error_budgets_are_hard under 100 concurrent callers" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    var sampled_buf: [32]u8 = undefined;
    const sampled_id = try findSampledId(&sampled_buf);
    trace_policy.resetForTest();

    var counts: EmitCounts = .{};
    var threads: [CONCURRENT_CALLERS]std.Thread = undefined;
    for (&threads) |*thread| {
        thread.* = try std.Thread.spawn(.{}, exerciseConcurrentPolicy, .{ &counts, sampled_id });
    }
    for (threads) |thread| thread.join();

    try std.testing.expectEqual(@as(u32, 4), loadCount(&counts.runner_rejections));
    try std.testing.expectEqual(@as(u32, 4), loadCount(&counts.server_errors));
    try std.testing.expectEqual(@as(u32, 2), loadCount(&counts.sampled_generic));

    // Runner 5xx responses above used only the server bucket: the runner
    // rejection window for the same second still admits its full four.
    for (0..4) |_| {
        try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.runner_report, 409, "runner", 200));
    }
}

test "fixed windows reset forward but never regress on late completion" {
    trace_policy.resetForTest();
    defer trace_policy.resetForTest();

    _ = trace_policy.decide(.runner_report, 429, "r", 11);
    try std.testing.expectEqual(
        trace_policy.Decision{ .suppress = .runner_rejection_budget },
        trace_policy.decide(.runner_report, 429, "r", 10),
    );
    for (0..3) |_| try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.runner_report, 429, "r", 11));
    try std.testing.expectEqual(
        trace_policy.Decision{ .suppress = .runner_rejection_budget },
        trace_policy.decide(.runner_report, 429, "r", 11),
    );
    try std.testing.expectEqual(trace_policy.Decision.emit, trace_policy.decide(.runner_report, 429, "r", 12));
}

test "test_request_span_elapsed_ignores_wall_clock_adjustment" {
    // pin test: literal is the contract — these three are the elapsed-time arithmetic.
    try std.testing.expectEqual(@as(u64, 1_250), trace_policy.endEpochNanos(1_000, 500, 750));
    // pin test: literal is the contract
    try std.testing.expectEqual(@as(u64, 1_000), trace_policy.endEpochNanos(1_000, 750, 500));
    try std.testing.expectEqual(std.math.maxInt(u64), trace_policy.endEpochNanos(std.math.maxInt(u64) - 1, 0, 10));
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

test "runner success chatter is suppressed before enqueue" {
    const route: router.Route = .{ .runner_activity = "lease" };
    try std.testing.expectEqual(trace_policy.Decision{ .suppress = .noisy_route }, trace_policy.decide(route, 200, "id", 30));
}
