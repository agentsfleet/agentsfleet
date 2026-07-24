const std = @import("std");
const metrics = @import("metrics_otel.zig");
const metrics_render = @import("metrics_render.zig");

const PRODUCER_COUNT: usize = 100;
const INCREMENTS_PER_PRODUCER: usize = 1_000;

const Producer = struct {
    fn run(producer_index: usize) void {
        const signal = metrics.SIGNALS[producer_index % metrics.SIGNALS.len];
        const reason = metrics.DISCARD_REASONS[producer_index % metrics.DISCARD_REASONS.len];
        for (0..INCREMENTS_PER_PRODUCER) |_| {
            metrics.recordDiscard(signal, reason, 1);
        }
    }
};

test "test_otlp_self_metrics_are_concurrent_and_exact" {
    metrics.resetForTest();
    defer metrics.resetForTest();

    var threads: [PRODUCER_COUNT]std.Thread = undefined;
    var started: usize = 0;
    errdefer for (threads[0..started]) |thread| thread.join();
    for (0..PRODUCER_COUNT) |producer_index| {
        threads[producer_index] = try std.Thread.spawn(.{}, Producer.run, .{producer_index});
        started += 1;
    }
    for (threads) |thread| thread.join();

    const actual = metrics.snapshot();
    var expected = [_][metrics.DISCARD_REASONS.len]u64{
        [_]u64{0} ** metrics.DISCARD_REASONS.len,
    } ** metrics.SIGNALS.len;
    for (0..PRODUCER_COUNT) |producer_index| {
        const signal_index = producer_index % metrics.SIGNALS.len;
        const reason_index = producer_index % metrics.DISCARD_REASONS.len;
        expected[signal_index][reason_index] += INCREMENTS_PER_PRODUCER;
    }
    try std.testing.expectEqualDeep(expected, actual.discarded);
}

test "test_otlp_self_metrics_render_fixed_labels" {
    metrics.resetForTest();
    defer metrics.resetForTest();

    metrics.setQueueDepth(.logs, 7);
    metrics.setQueueDepth(.traces, 8);
    metrics.setQueueDepth(.metrics, 9);
    metrics.recordDiscard(.logs, .ring_full, 2);
    metrics.recordDiscard(.traces, .partial_rejected, 3);
    metrics.recordDiscard(.metrics, .export_uncertain, 4);

    const body = try metrics_render.renderPrometheus(std.testing.allocator, true);
    defer std.testing.allocator.free(body);

    try std.testing.expectEqual(@as(usize, 5), std.mem.count(u8, body, metrics.QUEUE_DEPTH_NAME));
    try std.testing.expectEqual(@as(usize, 20), std.mem.count(u8, body, metrics.DISCARDED_NAME));
    try std.testing.expect(std.mem.containsAtLeast(
        u8,
        body,
        1,
        "agentsfleet_otlp_queue_depth{signal=\"logs\"} 7\n",
    ));
    try std.testing.expect(std.mem.containsAtLeast(
        u8,
        body,
        1,
        "agentsfleet_otlp_entries_discarded_total{signal=\"traces\",reason=\"partial_rejected\"} 3\n",
    ));
    try std.testing.expect(std.mem.containsAtLeast(
        u8,
        body,
        1,
        "agentsfleet_otlp_entries_discarded_total{signal=\"metrics\",reason=\"export_uncertain\"} 4\n",
    ));
}
