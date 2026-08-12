const std = @import("std");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const families = @import("otel_metrics_families.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const semconv = @import("semconv.zig");

const Mode = @import("../state/tenant_provider.zig").Mode;
const POSTURE: Mode = .platform;
const MODEL = "claude-opus-4-8";

fn sumSample(value: i64, charge: semconv.ChargeClass) payload.Sample {
    var s = payload.newSample(.credit_consumed, value);
    _ = payload.addClosedLabel(&s, semconv.ATTR_CHARGE_TYPE, charge);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.setDynamicLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
    return s;
}

fn histSample(value: i64) payload.Sample {
    var s = payload.newSample(.invoke_agent_duration, value);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, POSTURE);
    _ = payload.setDynamicLabel(&s, semconv.ATTR_REQUEST_MODEL, MODEL);
    return s;
}

test "test_aggregates_sum_per_window: same-labelset sums coalesce to one series" {
    var agg = aggregate.Aggregator.init();
    var i: usize = 0;
    while (i < 5) : (i += 1) agg.add(sumSample(10, semconv.ChargeClass.receive));

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(usize, 1), series.len);
    try std.testing.expectEqual(@as(i64, 50), series[0].sum_value); // 5 × 10
}

test "test_aggregates_histogram_per_window: observations merge into one histogram series" {
    var agg = aggregate.Aggregator.init();
    // Milliseconds against the pinned agent-duration bounds (100, 200, 400, …).
    agg.add(histSample(7)); // bucket 0 (<=100ms)
    agg.add(histSample(370)); // bucket 2 ((200, 400]ms)
    agg.add(histSample(8)); // bucket 0

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 3), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 385), series[0].hist_sum); // 7+370+8
    try std.testing.expectEqual(@as(u64, 2), series[0].bucket_counts[0]); // 7, 8
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[2]); // 370
}

test "histogram clamps a negative observation to bucket 0 and adds 0 to the sum" {
    var agg = aggregate.Aggregator.init();
    agg.add(histSample(-5)); // e.g. clock-skew wall_ms
    agg.add(histSample(370)); // bucket 2 ((200, 400]ms)
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(u64, 2), series[0].hist_count);
    try std.testing.expectEqual(@as(i64, 370), series[0].hist_sum); // -5 clamped to 0, + 370
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[0]); // -5 → bucket 0
    try std.testing.expectEqual(@as(u64, 1), series[0].bucket_counts[2]); // 370 → bucket 2
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
    agg.add(sumSample(10, semconv.ChargeClass.receive));
    agg.add(sumSample(20, semconv.ChargeClass.renewal));
    try std.testing.expectEqual(@as(usize, 2), agg.count);
}

fn dynamicSample(value: i64, model: []const u8) payload.Sample {
    var s = payload.newSample(.credit_consumed, value);
    _ = payload.setDynamicLabel(&s, semconv.ATTR_REQUEST_MODEL, model);
    return s;
}

test "test_aggregator_full_drops_and_counts: distinct series beyond the cap are dropped + counted" {
    var agg = aggregate.Aggregator.init();
    var buf: [24]u8 = undefined;
    const overflow: usize = 10;
    var i: usize = 0;
    while (i < aggregate.MAX_SERIES + overflow) : (i += 1) {
        agg.add(dynamicSample(1, try std.fmt.bufPrint(&buf, "model-{d}", .{i})));
    }
    try std.testing.expectEqual(aggregate.MAX_SERIES, agg.count);
    try std.testing.expectEqual(@as(u64, overflow), agg.dropped);
}

test "test_aggregator_collision_probe: bucket-colliding identities stay distinct series" {
    // Deterministically find two identity-distinct samples whose hashes land
    // in the same bucket (the hash seed is a fixed constant, so the search
    // result never varies), then prove the open-addressed probe keeps them as
    // separate series with separate values — no cross-series merge.
    // Far larger than the bucket table, so a collider provably exists inside it.
    const COLLISION_SEARCH_BOUND: usize = 100_000;
    var probe_buf: [24]u8 = undefined;
    const base = dynamicSample(5, "model-base");
    const target_bucket = aggregate.testIdentityBucket(base);
    var i: usize = 0;
    var collider: ?payload.Sample = null;
    while (i < COLLISION_SEARCH_BOUND) : (i += 1) {
        const candidate = dynamicSample(7, try std.fmt.bufPrint(&probe_buf, "model-{d}", .{i}));
        if (aggregate.testIdentityBucket(candidate) == target_bucket) {
            collider = candidate;
            break;
        }
    }
    // The bucket table is far smaller than the search bound, so a collider
    // always exists within it.
    try std.testing.expect(collider != null);

    var agg = aggregate.Aggregator.init();
    agg.add(base);
    agg.add(collider.?);
    agg.add(base); // folds onto the first series, not the collider's
    try std.testing.expectEqual(@as(usize, 2), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(i64, 10), series[0].sum_value);
    try std.testing.expectEqual(@as(i64, 7), series[1].sum_value);
}

test "a fresh aggregator starts empty (per-window reset)" {
    const agg = aggregate.Aggregator.init();
    try std.testing.expectEqual(@as(usize, 0), agg.count);
    try std.testing.expectEqual(@as(u64, 0), agg.dropped);
}

// ── §1 — a gauge is a level, not a running total ────────────────────────────

fn gaugeSample(value: i64) payload.Sample {
    // A live gauge identity so kind dispatch takes the real gauge path.
    return payload.newSample(.api_in_flight_requests, value);
}

// Dimension 1.1 — the kind enumeration carries a gauge variant distinct from
// sum and histogram.
test "test_metric_kind_admits_gauge" {
    const fields = @typeInfo(families.MetricKind).@"enum".fields;
    try std.testing.expectEqual(@as(usize, 3), fields.len);
    var has_gauge = false;
    inline for (fields) |f| {
        if (comptime std.mem.eql(u8, f.name, "gauge")) has_gauge = true;
    }
    try std.testing.expect(has_gauge);
    try std.testing.expect(families.MetricKind.gauge != .sum);
    try std.testing.expect(families.MetricKind.gauge != .histogram);
    // And the registry actually uses it: at least one declared level.
    try std.testing.expectEqual(families.MetricKind.gauge, families.metaFor(.api_in_flight_requests).kind);
}

// Dimension 1.2 — ten samples of one gauge label set fold to the newest value,
// never their sum: ten samples ending at seven yield seven, not seventy.
test "test_gauge_folds_to_last_value" {
    var agg = aggregate.Aggregator.init();
    var i: usize = 0;
    while (i < 10) : (i += 1) agg.add(gaugeSample(7));

    try std.testing.expectEqual(@as(usize, 1), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    try std.testing.expectEqual(@as(i64, 7), series[0].sum_value); // pin test: literal is the contract — 7, not 70
}

// Dimension 1.3 — a sum and a gauge sharing a flush window each fold by their
// own rule: the sum totals, the gauge takes the newest observation.
test "test_mixed_kinds_fold_independently" {
    var agg = aggregate.Aggregator.init();
    agg.add(sumSample(10, semconv.ChargeClass.receive));
    agg.add(gaugeSample(3));
    agg.add(sumSample(20, semconv.ChargeClass.receive));
    agg.add(gaugeSample(9));

    try std.testing.expectEqual(@as(usize, 2), agg.count);
    var buf: [aggregate.MAX_SERIES]payload.Series = undefined;
    const series = agg.toSeries(&buf);
    for (series) |s| {
        switch (families.metaFor(s.id).kind) {
            .sum => try std.testing.expectEqual(@as(i64, 30), s.sum_value), // 10 + 20
            .gauge => try std.testing.expectEqual(@as(i64, 9), s.sum_value), // newest, not 12
            .histogram => return error.UnexpectedKind,
        }
    }
}

// ── §2 — the series ceiling is derived, never chosen ────────────────────────

// Dimension 2.1 — the ceiling equals the sum of declared terms, and the
// runtime term is re-derived here independently from the same declarations,
// so changing any declaration changes the ceiling with it.
test "test_series_ceiling_is_derived_from_declarations" {
    try std.testing.expectEqual(families.COST_SERIES_BUDGET + families.RUNTIME_FIXED_SERIES, families.MAX_SERIES);
    try std.testing.expectEqual(families.MAX_SERIES, aggregate.MAX_SERIES);

    // Independent recomputation of the runtime term from the registry.
    const recomputed = comptime blk: {
        var total: usize = 0;
        for (0..families.METRIC_ID_COUNT) |i| {
            const meta = families.metaFor(@enumFromInt(i));
            if (!meta.cost and !meta.streamed) total += meta.max_series;
        }
        break :blk total;
    };
    try std.testing.expectEqual(recomputed, families.RUNTIME_FIXED_SERIES);
    try std.testing.expect(recomputed > 0);

    // The streamed term reuses the slot table's own capacity as its bound.
    const recomputed_streamed = comptime blk: {
        var total: usize = 0;
        for (0..families.METRIC_ID_COUNT) |i| {
            const meta = families.metaFor(@enumFromInt(i));
            if (meta.streamed) total += meta.max_series;
        }
        break :blk total;
    };
    try std.testing.expectEqual(recomputed_streamed, families.STREAMED_SERIES_WORST_CASE);
}

// Dimension 2.2 — the declared worst case sits under the hard cap. The
// enforcing assertion is COMPTIME in otel_metrics_families.zig: a declaration
// set whose worst case exceeds AGGREGATOR_HARD_CAP fails the BUILD before any
// deployment, so this test documents and pins the constants' relation rather
// than re-enforcing it at runtime.
test "test_declared_worst_case_fits_under_ceiling" {
    comptime std.debug.assert(families.MAX_SERIES <= families.AGGREGATOR_HARD_CAP);
    try std.testing.expect(families.MAX_SERIES <= families.AGGREGATOR_HARD_CAP);
    // Headroom is real: the cost sub-budget alone does not exhaust the cap,
    // so runtime growth is the only thing that can approach it — and it fails
    // the build there, never sheds series at runtime.
    try std.testing.expect(families.COST_SERIES_BUDGET < families.AGGREGATOR_HARD_CAP);
}

// Dimension 2.3 — the attribution budget derives from the cost sub-budget the
// registry declares, not from the widened total ceiling, so adding runtime
// families provably cannot shrink it.
test "test_attribution_budget_survives_family_growth" {
    try std.testing.expectEqual(semconv.modelAttributionCap(families.COST_SERIES_BUDGET), cardinality.ATTRIBUTION_CAP);
    try std.testing.expect(cardinality.ATTRIBUTION_CAP > 0);
    // The derivation's input is the cost sub-budget alone: RUNTIME_FIXED_SERIES
    // does not appear in it, so the cap computed from the pre-widening budget
    // and the post-widening budget are the same number.
    try std.testing.expectEqual(
        semconv.modelAttributionCap(families.MAX_SERIES - families.RUNTIME_FIXED_SERIES),
        cardinality.ATTRIBUTION_CAP,
    );
}
