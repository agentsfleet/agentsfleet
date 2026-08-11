//! OTLP-JSON metric serialization for the Grafana Cloud Mimir exporter: the
//! fixed-size `Sample` input type, the aggregated `Series` type, and the
//! per-series serializer. Family identity lives in `otel_metrics_families.zig`
//! — this file only knows how to put a declared family on the wire.
//!
//! Labels are interned: a `Label` is a pair of indices into the comptime
//! key/value tables in otel_metrics_dims.zig. Exactly one label per sample may
//! carry a caller-supplied value (request model, runner id), riding the
//! sample's single inline buffer; serialization resolves indices back to the
//! same strings, so the wire bytes are unchanged by interning.
//!
//! Evented families are DELTA: the flush coalesces a window's samples into
//! one `Series` per (metric, labelset) — see otel_metrics_aggregate.zig — and
//! a Fly-deployed OTel Collector (deltatocumulative) converts before Mimir.
//! Snapshot counters export as CUMULATIVE sums stamped with the process-start
//! time; gauges carry only the flush instant. Duration observations stay
//! integers in their source unit even where the metric declares `s`: every
//! pinned bound is a whole multiple of the divisor, so integer bucketing is
//! exact and the seconds conversion happens once, at serialization, with no
//! floating-point arithmetic anywhere.

const std = @import("std");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const dims = @import("otel_metrics_dims.zig");
const otlp_config = @import("otlp/config.zig");

pub const MetricId = families.MetricId;
const MetricMeta = families.MetricMeta;

// ---------------------------------------------------------------------------
// Fixed-size sample (no heap; copied by value into the ring like SpanEntry).
// ---------------------------------------------------------------------------

/// Widest live labelset is token usage: operation name, provider, model, token
/// type, posture.
pub const MAX_LABELS: usize = 5;
/// Longest caller-supplied label value (request model, runner id) a sample
/// can carry inline; longer values are omitted and counted, never truncated.
pub const MAX_LABEL_VAL: usize = 64;

/// Comptime bound the compact sample layout is held to: the ring holds 1024
/// samples and the flush thread stacks one accumulator per series.
pub const SAMPLE_SIZE_BOUND: usize = 128;

/// Buckets = the widest pinned bound table plus the trailing +Inf bucket;
/// each metric serializes its own `meta.bounds.len + 1` slice of the array.
pub const N_BUCKETS: usize = semconv.MAX_BUCKET_BOUNDS + 1;

/// Sentinel `val_idx` routing a label's value to the sample's inline dynamic
/// buffer instead of the interned table (otel_metrics_dims.zig).
const DYNAMIC_VALUE_INDEX: u16 = std.math.maxInt(u16);

comptime {
    std.debug.assert(dims.VALUES.len < DYNAMIC_VALUE_INDEX);
}

/// Resolve one label's key string (test/serializer surface).
pub fn labelKey(label: Label) []const u8 {
    return dims.KEYS[label.key_idx];
}

/// Resolve one label's value string against its sample's dynamic buffer.
pub fn labelValue(label: Label, dynamic: []const u8) []const u8 {
    return if (label.val_idx == DYNAMIC_VALUE_INDEX) dynamic else dims.VALUES[label.val_idx];
}

/// One interned label — the index pair is its whole aggregation identity.
pub const Label = struct {
    key_idx: u8,
    val_idx: u16,
};

/// One emitted measurement, the input to flush-time aggregation. No timestamp:
/// the flush window stamps the aggregated dataPoint, not the sample.
pub const Sample = struct {
    /// Sum delta, gauge level, or the observed value for a histogram.
    value: i64,
    /// The at-most-one caller-supplied label value (model, runner id).
    dynamic: [MAX_LABEL_VAL]u8,
    labels: [MAX_LABELS]Label,
    id: MetricId,
    label_count: u8,
    dynamic_len: u8,
};

comptime {
    std.debug.assert(@sizeOf(Sample) <= SAMPLE_SIZE_BOUND);
}

/// An aggregated series for one flush window: all same-`(id, labels)` samples
/// coalesced. Sums and gauges use `sum_value`; histograms use `hist_*` +
/// `bucket_counts`; `dynamic` backs the sample's inline dynamic label value.
pub const Series = struct {
    id: MetricId,
    labels: []const Label,
    dynamic: []const u8,
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

/// Wire timestamps for one batch: delta streams span the flush window,
/// cumulative streams start at process start, gauges carry only the instant.
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
        // SAFETY: only slots [0, label_count) are ever read, and the
        // add*Label writers fill each slot before bumping the count.
        .labels = undefined,
        .label_count = 0,
        // SAFETY: only bytes [0, dynamic_len) are ever read, and
        // setDynamicLabel copies them before setting the length.
        .dynamic = undefined,
        .dynamic_len = 0,
    };
}

/// Snapshot counters are u64; Sample.value is i64. Saturate rather than trap:
/// telemetry, not money.
pub fn satCast(value: u64) i64 {
    return @intCast(@min(value, std.math.maxInt(i64)));
}

/// Attach a label whose value is one of the declared closed values. False
/// (label dropped) when the sample is full or the value is not in the closed
/// table — the caller counts an omission rather than exporting it.
pub fn addClosedLabel(sample: *Sample, comptime key: []const u8, val: []const u8) bool {
    if (sample.label_count >= MAX_LABELS) return false;
    const val_idx = dims.runtimeValueIndex(val) orelse return false;
    sample.labels[sample.label_count] = .{ .key_idx = comptime dims.keyIndexOf(key), .val_idx = val_idx };
    sample.label_count += 1;
    return true;
}

/// Sibling of addClosedLabel with validation moved to the build: a misspelled
/// key or value is a compile error, not a false return the caller must count.
pub fn addInternedLabel(sample: *Sample, comptime key: []const u8, comptime val: []const u8) bool {
    if (sample.label_count >= MAX_LABELS) return false;
    sample.labels[sample.label_count] = .{ .key_idx = comptime dims.keyIndexOf(key), .val_idx = comptime dims.internedValueIndex(val) };
    sample.label_count += 1;
    return true;
}

/// Attach the sample's single caller-supplied label value. False when the
/// sample is full, the value would overflow the inline buffer (the caller
/// counts an omission — never truncates), or a dynamic label already exists.
pub fn setDynamicLabel(sample: *Sample, comptime key: []const u8, val: []const u8) bool {
    if (sample.label_count >= MAX_LABELS) return false;
    if (val.len > MAX_LABEL_VAL) return false;
    for (sample.labels[0..sample.label_count]) |label| {
        if (label.val_idx == DYNAMIC_VALUE_INDEX) return false;
    }
    @memcpy(sample.dynamic[0..val.len], val);
    sample.dynamic_len = @intCast(val.len);
    sample.labels[sample.label_count] = .{ .key_idx = comptime dims.keyIndexOf(key), .val_idx = DYNAMIC_VALUE_INDEX };
    sample.label_count += 1;
    return true;
}

/// True when `val` would survive `setDynamicLabel` intact — lets a caller
/// omit an attribute (and count it) before mutating the sample.
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

fn appendAttributes(list: *std.ArrayList(u8), alloc: std.mem.Allocator, labels: []const Label, dynamic: []const u8) !void {
    try list.appendSlice(alloc, "\"attributes\":[");
    for (labels, 0..) |lbl, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        // Both key and value go through json.fmt (which adds the quotes and
        // escapes the interior) — keys are trusted consts today, but routing
        // them through json.fmt keeps the whole attribute escape-safe and
        // consistent with the value + the traces/logs serializers.
        try list.print(alloc, "{{\"key\":{f},\"value\":{{\"stringValue\":{f}}}}}", .{
            std.json.fmt(labelKey(lbl), .{}),
            std.json.fmt(labelValue(lbl, dynamic), .{}),
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
    try appendAttributes(list, alloc, series.labels, series.dynamic);
    try list.print(alloc, ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",", .{ start_ns, times.now_ns });
    try appendPointValue(list, alloc, series.sum_value, meta.scale);
    try list.appendSlice(alloc, DATA_POINTS_SUFFIX);
}

/// A gauge is a level: no temporality, no start time — only the instant the
/// flush observed it. Serialized in the native OTLP gauge shape so a reader
/// can tell a level from a counter by its type alone.
fn appendGauge(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, times: WireTimes) !void {
    try list.appendSlice(alloc, "\"gauge\":{\"dataPoints\":[{");
    try appendAttributes(list, alloc, series.labels, series.dynamic);
    try list.print(alloc, ",\"timeUnixNano\":\"{d}\",", .{times.now_ns});
    try appendPointValue(list, alloc, series.sum_value, meta.scale);
    try list.appendSlice(alloc, DATA_POINTS_SUFFIX);
}

fn appendHistogram(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, times: WireTimes) !void {
    try list.print(alloc, "\"histogram\":{{\"aggregationTemporality\":{d},\"dataPoints\":[{{", .{AGGREGATION_TEMPORALITY_DELTA});
    try appendAttributes(list, alloc, series.labels, series.dynamic);
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

/// What the extra-append hook did: appended series join the export count the
/// backend's partial-rejection reply is validated against; series shed at the
/// fixed payload arena must surface as a discard rather than vanish.
pub const ExtraAppendResult = struct { appended: usize = 0, shed: usize = 0 };

/// Serialize aggregated series into one complete OTLP-JSON metrics envelope,
/// sharing the resource + scope serializer with the logs and traces signals.
/// `extra` appends any additional metric objects (the streamed per-runner
/// families) inside the same envelope; pass null when there are none.
pub const ExtraAppendFn = *const fn (
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    times: WireTimes,
    wrote_any: bool,
) anyerror!ExtraAppendResult;

pub const SerializedEnvelope = struct {
    body: []u8,
    extra: ExtraAppendResult,
};

pub fn serializeSeries(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    series: []const Series,
    times: WireTimes,
    extra: ?ExtraAppendFn,
) !SerializedEnvelope {
    var list: std.ArrayList(u8) = .empty;
    errdefer list.deinit(alloc);
    try otlp_config.appendEnvelopePrefix(&list, alloc, cfg, "resourceMetrics", "scopeMetrics", "metrics");
    for (series, 0..) |s, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try appendSeriesMetric(&list, alloc, s, times);
    }
    var extra_result: ExtraAppendResult = .{};
    if (extra) |append_extra| {
        extra_result = try append_extra(&list, alloc, times, series.len > 0);
    }
    try list.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);
    return .{ .body = try list.toOwnedSlice(alloc), .extra = extra_result };
}
