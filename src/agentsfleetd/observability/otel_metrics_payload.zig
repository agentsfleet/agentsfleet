//! OTLP-JSON metric serialization for the Grafana Cloud Mimir exporter.
//! Holds the fixed-size `Sample` input type, the aggregated `Series` type,
//! and the per-series serializer the flush loop calls. Family identity
//! (names, kinds, units, temporality, ceiling arithmetic) lives in
//! `otel_metrics_families.zig` — this file only knows how to put a declared
//! family on the wire.
//!
//! Evented families are DELTA: the flush coalesces a window's samples into
//! one `Series` per (metric, labelset) — see otel_metrics_aggregate.zig — and
//! a Fly-deployed OTel Collector (deltatocumulative) converts before Mimir.
//! Snapshot counters export as CUMULATIVE sums stamped with the process-start
//! time; gauges carry only the flush instant.
//!
//! Duration observations are carried and bucketed as integers in their source
//! unit (milliseconds or nanoseconds) even where the metric declares `s`:
//! every pinned bound is a whole multiple of the divisor, so integer
//! bucketing is exact and the seconds conversion happens once, at
//! serialization, with no floating-point arithmetic anywhere.

const std = @import("std");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const otlp_config = @import("otlp/config.zig");

pub const MetricId = families.MetricId;
const MetricMeta = families.MetricMeta;

// ---------------------------------------------------------------------------
// Fixed-size sample (no heap; copied by value into the ring like SpanEntry).
// ---------------------------------------------------------------------------

/// Widest live labelset is token usage: operation name, provider, model, token
/// type, posture.
pub const MAX_LABELS: usize = 5;
/// Longest live key is `agentsfleet.billing.charge.type` (31 bytes).
pub const MAX_LABEL_KEY: usize = 32;
pub const MAX_LABEL_VAL: usize = 64;

/// Buckets = the widest pinned bound table plus the trailing +Inf bucket. The
/// tables differ in length upstream, so this cuts to the longest; each metric is
/// serialized against its own `meta.bounds.len + 1` slice of the array.
pub const N_BUCKETS: usize = semconv.MAX_BUCKET_BOUNDS + 1;

pub const Label = struct {
    key: [MAX_LABEL_KEY]u8,
    key_len: u8,
    val: [MAX_LABEL_VAL]u8,
    val_len: u8,
};

/// One emitted measurement, the input to flush-time aggregation. No timestamp:
/// the flush window stamps the aggregated dataPoint, not the individual sample.
pub const Sample = struct {
    id: MetricId,
    /// Sum delta, gauge level, or the observed value for a histogram.
    value: i64,
    labels: [MAX_LABELS]Label,
    label_count: u8,
};

/// An aggregated series for one flush window: all same-`(id, labels)` samples
/// coalesced. Sums and gauges use `sum_value`; histograms use `hist_*` +
/// `bucket_counts`.
pub const Series = struct {
    id: MetricId,
    labels: []const Label,
    sum_value: i64,
    hist_count: u64,
    hist_sum: i64,
    bucket_counts: []const u64,
};

/// OTLP AggregationTemporality enum: 1 = DELTA, 2 = CUMULATIVE.
const AGGREGATION_TEMPORALITY_DELTA: u8 = 1;
const AGGREGATION_TEMPORALITY_CUMULATIVE: u8 = 2;

/// A bare unsigned integer — the JSON number form for a count-unit histogram
/// bound and for its sum.
const FMT_UNSIGNED = "{d}";

/// Closes one dataPoint object, its dataPoints array, and the enclosing
/// sum/gauge object — the shared suffix of every single-point metric body.
const DATA_POINTS_SUFFIX = "}]}";

/// Wire timestamps for one serialized batch. Delta streams span the flush
/// window; cumulative streams start at process start; gauges carry only the
/// flush instant.
pub const WireTimes = struct {
    window_start_ns: u64,
    process_start_ns: u64,
    now_ns: u64,
};

/// Initialize an empty sample for `id` with `value`.
pub fn newSample(id: MetricId, value: i64) Sample {
    return .{
        .id = id,
        .value = value,
        // SAFETY: indices [0, label_count) are written by addLabel before any
        // reader (aggregation) touches them; slots past label_count are never read.
        .labels = undefined,
        .label_count = 0,
    };
}

/// Append a label to a sample. Returns false (and drops the label) when full or
/// when key/value would overflow their fixed buffers — never partially writes.
/// A dropped value is the caller's signal to count an attribute omission rather
/// than export a truncated value that reads as a different model or provider.
pub fn addLabel(sample: *Sample, key: []const u8, val: []const u8) bool {
    if (sample.label_count >= MAX_LABELS) return false;
    if (key.len > MAX_LABEL_KEY or val.len > MAX_LABEL_VAL) return false;
    const idx = sample.label_count;
    sample.labels[idx].key_len = @intCast(key.len);
    @memcpy(sample.labels[idx].key[0..key.len], key);
    sample.labels[idx].val_len = @intCast(val.len);
    @memcpy(sample.labels[idx].val[0..val.len], val);
    sample.label_count += 1;
    return true;
}

/// True when `val` would survive `addLabel` intact. Lets a caller decide to omit
/// an attribute (and count it) before mutating the sample.
pub fn valueFits(val: []const u8) bool {
    return val.len <= MAX_LABEL_VAL;
}

/// Index of the bucket a value falls in: first bound it is <=, else the
/// trailing +Inf bucket (== bounds.len).
pub fn bucketIndex(value: u64, bounds: []const u64) usize {
    for (bounds, 0..) |bound, i| {
        if (value <= bound) return i;
    }
    return bounds.len;
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

fn appendAttributes(list: *std.ArrayList(u8), alloc: std.mem.Allocator, labels: []const Label) !void {
    try list.appendSlice(alloc, "\"attributes\":[");
    for (labels, 0..) |lbl, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        // Both key and value go through json.fmt (which adds the quotes and
        // escapes the interior) — keys are trusted consts today, but routing
        // them through json.fmt keeps the whole attribute escape-safe and
        // consistent with the value + the traces/logs serializers.
        try list.print(alloc, "{{\"key\":{f},\"value\":{{\"stringValue\":{f}}}}}", .{
            std.json.fmt(lbl.key[0..lbl.key_len], .{}),
            std.json.fmt(lbl.val[0..lbl.val_len], .{}),
        });
    }
    try list.appendSlice(alloc, "]");
}

/// Print an integer source-unit quantity as the seconds JSON number the `s`
/// unit declares — integer part, then exactly as many fractional digits as
/// the divisor has zeros. No float arithmetic: 10 ms serializes as `0.010`,
/// 81920 ms as `81.920`, 1_500_000_000 ns as `1.500000000`.
fn printScaled(list: *std.ArrayList(u8), alloc: std.mem.Allocator, value: u64, scale: families.Scale) !void {
    switch (scale) {
        .none => try list.print(alloc, FMT_UNSIGNED, .{value}),
        .millis_to_seconds => try list.print(alloc, "{d}.{d:0>3}", .{ value / semconv.MILLIS_PER_SECOND, value % semconv.MILLIS_PER_SECOND }),
        .nanos_to_seconds => try list.print(alloc, "{d}.{d:0>9}", .{ value / semconv.NANOS_PER_SECOND, value % semconv.NANOS_PER_SECOND }),
    }
}

/// The numeric field of one dataPoint: `asInt` (string form) for unscaled
/// integers, `asDouble` (JSON number, exact decimal) for scaled quantities.
fn appendPointValue(list: *std.ArrayList(u8), alloc: std.mem.Allocator, value: i64, scale: families.Scale) !void {
    const magnitude: u64 = @intCast(@max(value, 0));
    if (scale == .none) {
        try list.print(alloc, "\"asInt\":\"{d}\"", .{value});
        return;
    }
    try list.appendSlice(alloc, "\"asDouble\":");
    try printScaled(list, alloc, magnitude, scale);
}

fn appendSum(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, times: WireTimes) !void {
    const temporality: u8 = switch (meta.temporality) {
        .delta => AGGREGATION_TEMPORALITY_DELTA,
        .cumulative => AGGREGATION_TEMPORALITY_CUMULATIVE,
    };
    const start_ns = switch (meta.temporality) {
        .delta => times.window_start_ns,
        .cumulative => times.process_start_ns,
    };
    try list.print(
        alloc,
        "\"sum\":{{\"aggregationTemporality\":{d},\"isMonotonic\":{s},\"dataPoints\":[{{",
        .{ temporality, if (meta.monotonic) "true" else "false" },
    );
    try appendAttributes(list, alloc, series.labels);
    try list.print(alloc, ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",", .{ start_ns, times.now_ns });
    try appendPointValue(list, alloc, series.sum_value, meta.scale);
    try list.appendSlice(alloc, DATA_POINTS_SUFFIX);
}

/// A gauge is a level: no temporality, no start time — only the instant the
/// flush observed it. Serialized in the native OTLP gauge shape so a reader
/// can tell a level from a counter by its type alone.
fn appendGauge(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, times: WireTimes) !void {
    try list.appendSlice(alloc, "\"gauge\":{\"dataPoints\":[{");
    try appendAttributes(list, alloc, series.labels);
    try list.print(alloc, ",\"timeUnixNano\":\"{d}\",", .{times.now_ns});
    try appendPointValue(list, alloc, series.sum_value, meta.scale);
    try list.appendSlice(alloc, DATA_POINTS_SUFFIX);
}

fn appendHistogram(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, times: WireTimes) !void {
    try list.print(alloc, "\"histogram\":{{\"aggregationTemporality\":{d},\"dataPoints\":[{{", .{AGGREGATION_TEMPORALITY_DELTA});
    try appendAttributes(list, alloc, series.labels);
    try list.print(alloc, ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"count\":\"{d}\",\"sum\":", .{ times.window_start_ns, times.now_ns, series.hist_count });
    const sum_magnitude: u64 = @intCast(@max(series.hist_sum, 0));
    try printScaled(list, alloc, sum_magnitude, meta.scale);
    try list.appendSlice(alloc, ",\"bucketCounts\":[");
    for (series.bucket_counts[0 .. meta.bounds.len + 1], 0..) |bc, b| {
        if (b > 0) try list.appendSlice(alloc, ",");
        try list.print(alloc, "\"{d}\"", .{bc});
    }
    try list.appendSlice(alloc, "],\"explicitBounds\":[");
    for (meta.bounds, 0..) |bound, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try printScaled(list, alloc, bound, meta.scale);
    }
    try list.appendSlice(alloc, "]}]}");
}

/// Serialize one aggregated series as a complete OTLP `metric` JSON object,
/// appended to `list`. Caller writes the inter-object comma.
pub fn appendSeriesMetric(
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    series: Series,
    times: WireTimes,
) !void {
    const meta = families.metaFor(series.id);
    try list.print(alloc, "{{\"name\":\"{s}\",\"unit\":\"{s}\",", .{ meta.name, meta.unit });
    switch (meta.kind) {
        .sum => try appendSum(list, alloc, series, meta, times),
        .gauge => try appendGauge(list, alloc, series, meta, times),
        .histogram => try appendHistogram(list, alloc, series, meta, times),
    }
    try list.appendSlice(alloc, "}");
}

/// Serialize aggregated series into one complete OTLP-JSON metrics envelope,
/// sharing the resource + scope serializer with the logs and traces signals.
/// `extra` appends any additional metric objects (the streamed per-runner
/// families) inside the same envelope; pass null when there are none.
pub const ExtraAppendFn = *const fn (
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    times: WireTimes,
    wrote_any: bool,
) anyerror!bool;

pub fn serializeSeries(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    series: []const Series,
    times: WireTimes,
    extra: ?ExtraAppendFn,
) ![]u8 {
    var list: std.ArrayList(u8) = .empty;
    errdefer list.deinit(alloc);
    try otlp_config.appendEnvelopePrefix(&list, alloc, cfg, "resourceMetrics", "scopeMetrics", "metrics");
    for (series, 0..) |s, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try appendSeriesMetric(&list, alloc, s, times);
    }
    if (extra) |append_extra| {
        _ = try append_extra(&list, alloc, times, series.len > 0);
    }
    try list.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);
    return list.toOwnedSlice(alloc);
}
