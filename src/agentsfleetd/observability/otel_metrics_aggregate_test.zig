const std = @import("std");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");

fn sumSample(value: i64, workspace: []const u8) payload.Sample {
    var s = payload.newSample(.credit_drain, value);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, "standard");
    _ = payload.addLabel(&s, payload.LABEL_MODEL, "m");
    _ = payload.addLabel(&s, payload.LABEL_WORKSPACE, workspace);
    return s;
}

fn histSample(value: i64) payload.Sample {
    var s = payload.newSample(.run_duration, value);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, "standard");
    _ = payload.addLabel(&s, payload.LABEL_MODEL, "m");
    return s;
}

test "test_aggregates_sum_per_window: same-labelset sums coalesce to one series" {
    var agg = aggregate.Aggregator.init();
    var i: usize = 0;
    while (i < 5) : (i += 1) agg.add(sumSample(10, "ws-a"));

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(usize, 1), series.len);
    try std.testing.expectEqual(@as(i64, 50), series[0].sum_value); // 5 × 10
}

test "test_aggregates_histogram_per_window: observations merge into one histogram series" {
    var agg = aggregate.Aggregator.init();
    agg.add(histSample(7)); // bucket 1 (<=10)
    agg.add(histSample(37)); // bucket 3 ((25,50])
    agg.add(histSample(8)); // bucket 1

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 3), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 52), series[0].hist_sum); // 7+37+8
    try std.testing.expectEqual(@as(u64, 2), series[0].bucket_counts[1]); // 7, 8
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[3]); // 37
}

test "histogram clamps a negative observation to bucket 0 and adds 0 to the sum" {
    var agg = aggregate.Aggregator.init();
    agg.add(histSample(-5)); // e.g. clock-skew wall_ms
    agg.add(histSample(8)); // bucket 1
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 2), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 8), series[0].hist_sum); // -5 clamped to 0, + 8
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[0]); // -5 → bucket 0
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[1]); // 8 → bucket 1
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
    agg.add(sumSample(10, "ws-a"));
    agg.add(sumSample(20, "ws-b"));
    try std.testing.expectEqual(@as(usize, 2), agg.count);
}

test "test_registry_cap_drops_and_counts: distinct series beyond the cap are dropped + counted" {
    var agg = aggregate.Aggregator.init();
    var buf: [16]u8 = undefined;
    const overflow: usize = 10;
    var i: usize = 0;
    while (i < aggregate.MAX_SERIES + overflow) : (i += 1) {
        const ws = try std.fmt.bufPrint(&buf, "ws-{d}", .{i});
        agg.add(sumSample(1, ws));
    }
    try std.testing.expectEqual(aggregate.MAX_SERIES, agg.count);
    try std.testing.expectEqual(@as(u64, overflow), agg.dropped);
}

test "a fresh aggregator starts empty (per-window reset)" {
    const agg = aggregate.Aggregator.init();
    try std.testing.expectEqual(@as(usize, 0), agg.count);
    try std.testing.expectEqual(@as(u64, 0), agg.dropped);
}
