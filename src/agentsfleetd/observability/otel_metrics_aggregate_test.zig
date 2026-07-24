const std = @import("std");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const semconv = @import("semconv.zig");

const POSTURE = "platform";
const MODEL = "claude-opus-4-8";

fn sumSample(value: i64, charge: []const u8) payload.Sample {
    var s = payload.newSample(.credit_consumed, value);
    _ = payload.addLabel(&s, semconv.ATTR_CHARGE_TYPE, charge);
    _ = payload.addLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
    return s;
}

fn histSample(value: i64) payload.Sample {
    var s = payload.newSample(.invoke_agent_duration, value);
    _ = payload.addLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.addLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
    return s;
}

test "test_aggregates_sum_per_window: same-labelset sums coalesce to one series" {
    var agg = aggregate.Aggregator.init();
    var i: usize = 0;
    while (i < 5) : (i += 1) agg.add(sumSample(10, semconv.ChargeClass.receive.label()));

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(usize, 1), series.len);
    try std.testing.expectEqual(@as(i64, 50), series[0].sum_value); // 5 × 10
}

test "test_aggregates_histogram_per_window: observations merge into one histogram series" {
    var agg = aggregate.Aggregator.init();
    // Milliseconds against the pinned agent-duration bounds (10, 20, 40, …).
    agg.add(histSample(7)); // bucket 0 (<=10ms)
    agg.add(histSample(37)); // bucket 2 ((20, 40]ms)
    agg.add(histSample(8)); // bucket 0

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 3), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 52), series[0].hist_sum); // 7+37+8
    try std.testing.expectEqual(@as(u64, 2), series[0].bucket_counts[0]); // 7, 8
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[2]); // 37
}

test "histogram clamps a negative observation to bucket 0 and adds 0 to the sum" {
    var agg = aggregate.Aggregator.init();
    agg.add(histSample(-5)); // e.g. clock-skew wall_ms
    agg.add(histSample(37)); // bucket 2 ((20, 40]ms)
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 2), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 37), series[0].hist_sum); // -5 clamped to 0, + 37
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[0]); // -5 → bucket 0
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[2]); // 37 → bucket 2
}

test "histogram sum saturates instead of trapping on two maxInt observations" {
    var agg = aggregate.Aggregator.init();
    // A runner-saturated wall_ms reaches here as maxInt(i64); two in one window
    // would overflow a plain += and trap in ReleaseSafe. Saturating add caps it.
    agg.add(histSample(std.math.maxInt(i64)));
    agg.add(histSample(std.math.maxInt(i64)));
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 2), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, std.math.maxInt(i64)), series[0].hist_sum);
}

test "distinct label sets aggregate into distinct series" {
    var agg = aggregate.Aggregator.init();
    agg.add(sumSample(10, semconv.ChargeClass.receive.label()));
    agg.add(sumSample(20, semconv.ChargeClass.renewal.label()));
    try std.testing.expectEqual(@as(usize, 2), agg.count);
}

test "test_registry_cap_drops_and_counts: distinct series beyond the cap are dropped + counted" {
    var agg = aggregate.Aggregator.init();
    var buf: [16]u8 = undefined;
    const overflow: usize = 10;
    var i: usize = 0;
    while (i < aggregate.MAX_SERIES + overflow) : (i += 1) {
        const charge = try std.fmt.bufPrint(&buf, "charge-{d}", .{i});
        agg.add(sumSample(1, charge));
    }
    try std.testing.expectEqual(aggregate.MAX_SERIES, agg.count);
    try std.testing.expectEqual(@as(u64, overflow), agg.dropped);
}

test "a fresh aggregator starts empty (per-window reset)" {
    const agg = aggregate.Aggregator.init();
    try std.testing.expectEqual(@as(usize, 0), agg.count);
    try std.testing.expectEqual(@as(u64, 0), agg.dropped);
}
