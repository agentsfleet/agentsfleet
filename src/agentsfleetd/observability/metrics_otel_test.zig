const std = @import("std");
const metrics = @import("metrics_otel.zig");
const window = @import("otel_metrics_window_test.zig");

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

test "test_otlp_self_metrics_export_fixed_labels" {
    const alloc = std.testing.allocator;
    metrics.resetForTest();
    defer metrics.resetForTest();

    metrics.setQueueDepth(.logs, 7);
    metrics.setQueueDepth(.traces, 8);
    metrics.setQueueDepth(.metrics, 9);
    metrics.recordDiscard(.logs, .ring_full, 2);
    metrics.recordDiscard(.traces, .partial_rejected, 3);
    metrics.recordDiscard(.metrics, .export_uncertain, 4);

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // Every fixed cell is a live series: one queue-depth point per signal and
    // one discard point per (signal, reason) — zero cells included.
    try std.testing.expectEqual(metrics.SIGNALS.len, try window.countFamilyWith(body, metrics.QUEUE_DEPTH_NAME, &.{}));
    try std.testing.expectEqual(
        metrics.SIGNALS.len * metrics.DISCARD_REASONS.len,
        try window.countFamilyWith(body, metrics.DISCARDED_NAME, &.{}),
    );

    var sig_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    try std.testing.expectEqual(@as(i64, 7), try window.familyValueWith(body, metrics.QUEUE_DEPTH_NAME, &.{
        try window.attrFragment(&sig_buf, "signal", "logs"),
    }));
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, metrics.DISCARDED_NAME, &.{
        try window.attrFragment(&sig_buf, "signal", "traces"),
        try window.attrFragment(&reason_buf, "reason", "partial_rejected"),
    }));
    try std.testing.expectEqual(@as(i64, 4), try window.familyValueWith(body, metrics.DISCARDED_NAME, &.{
        try window.attrFragment(&sig_buf, "signal", "metrics"),
        try window.attrFragment(&reason_buf, "reason", "export_uncertain"),
    }));
}

// The omission collector walks a two-dimensional counter table. A transposed
// `[attribute][reason]` index would pair the wrong attribute with the wrong
// reason, and an operator chasing a gap in model coverage would be sent after
// the wrong cause. Deliberately distinct per-cell counts make a transposition
// produce a different window; equal counts would let it through.
test "test_otlp_attribute_omissions_export_exact_attribute_reason_pairs" {
    const alloc = std.testing.allocator;
    metrics.resetForTest();
    defer metrics.resetForTest();

    metrics.recordAttributeOmission(.provider_name, .unmapped_provider);
    for (0..2) |_| metrics.recordAttributeOmission(.request_model, .budget_exhausted);
    for (0..3) |_| metrics.recordAttributeOmission(.request_model, .value_too_long);

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // Label VALUES are the wire attribute keys, so the dashboard label reads as
    // the same string the OTLP payload would have carried.
    var attr_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, metrics.ATTRIBUTE_OMITTED_NAME, &.{
        try window.attrFragment(&attr_buf, "attribute", "gen_ai.provider.name"),
        try window.attrFragment(&reason_buf, "reason", "unmapped_provider"),
    }));
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, metrics.ATTRIBUTE_OMITTED_NAME, &.{
        try window.attrFragment(&attr_buf, "attribute", "gen_ai.request.model"),
        try window.attrFragment(&reason_buf, "reason", "budget_exhausted"),
    }));
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, metrics.ATTRIBUTE_OMITTED_NAME, &.{
        try window.attrFragment(&attr_buf, "attribute", "gen_ai.request.model"),
        try window.attrFragment(&reason_buf, "reason", "value_too_long"),
    }));

    // A pair that was never recorded must stay at zero rather than inherit a
    // neighbouring cell's count — the other half of the transposition guard.
    try std.testing.expectEqual(@as(i64, 0), try window.familyValueWith(body, metrics.ATTRIBUTE_OMITTED_NAME, &.{
        try window.attrFragment(&attr_buf, "attribute", "gen_ai.provider.name"),
        try window.attrFragment(&reason_buf, "reason", "budget_exhausted"),
    }));

    // Every cell exports, so a zeroed counter is still a visible series.
    try std.testing.expectEqual(
        metrics.OMITTED_ATTRIBUTES.len * metrics.OMISSION_REASONS.len,
        try window.countFamilyWith(body, metrics.ATTRIBUTE_OMITTED_NAME, &.{}),
    );
}
