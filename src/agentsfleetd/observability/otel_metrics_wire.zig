//! OTLP JSON serialization for the metric exporter: one series to its
//! `sum`/`gauge`/`histogram` body, and a batch to the resourceMetrics envelope.
//! Split from otel_metrics_payload.zig, which keeps sample construction and the
//! interned label layout — this file only renders what that one assembles.

const std = @import("std");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");
const otlp_config = @import("otlp/config.zig");
const payload = @import("otel_metrics_payload.zig");

const MetricMeta = families.MetricMeta;
const Label = payload.Label;
const Series = payload.Series;
const WireTimes = payload.WireTimes;

/// OTLP AggregationTemporality enum: 1 = DELTA, 2 = CUMULATIVE.
const AGGREGATION_TEMPORALITY_DELTA: u8 = 1;
const AGGREGATION_TEMPORALITY_CUMULATIVE: u8 = 2;

/// A bare unsigned integer — the JSON number form for a count-unit histogram
/// bound and for its sum.
const FMT_UNSIGNED = "{d}";

/// Closes one dataPoint object, its dataPoints array, and the enclosing
/// sum/gauge object — the shared suffix of every single-point metric body.
const DATA_POINTS_SUFFIX = "}]}";

fn appendAttributes(list: *std.ArrayList(u8), alloc: std.mem.Allocator, labels: []const Label, dynamic: []const u8) !void {
    try list.appendSlice(alloc, "\"attributes\":[");
    for (labels, 0..) |lbl, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        // Both key and value go through json.fmt (which adds the quotes and
        // escapes the interior) — keys are trusted consts today, but routing
        // them through json.fmt keeps the whole attribute escape-safe and
        // consistent with the value + the traces/logs serializers.
        try list.print(alloc, "{{\"key\":{f},\"value\":{{\"stringValue\":{f}}}}}", .{
            std.json.fmt(payload.labelKey(lbl), .{}),
            std.json.fmt(payload.labelValue(lbl, dynamic), .{}),
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
