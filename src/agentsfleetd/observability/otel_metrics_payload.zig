//! OTLP-JSON metric serialization for the Grafana Cloud Mimir exporter.
//! Holds the wire descriptor table (name/unit/kind/bounds resolved against
//! `semconv.zig`), the fixed-size `Sample` input type, the aggregated `Series`
//! type, and the per-series serializer the flush loop calls.
//!
//! Temporality is DELTA: the flush coalesces a window's samples into one
//! `Series` per (metric, labelset) — see otel_metrics_aggregate.zig — each
//! serialized as a single dataPoint. A Fly-deployed OTel Collector
//! (deltatocumulative) converts delta → cumulative before Mimir.
//!
//! Duration observations are carried and bucketed as integer milliseconds even
//! though the metric declares the `s` unit: every pinned bound is a whole
//! multiple of 10 ms, so integer bucketing is exact and the seconds conversion
//! happens once, at serialization, with no floating-point arithmetic anywhere.

const std = @import("std");
const semconv = @import("semconv.zig");
const otlp_config = @import("otlp/config.zig");

// ---------------------------------------------------------------------------
// Fixed-size sample (no heap; copied by value into the ring like SpanEntry).
// ---------------------------------------------------------------------------

/// Widest live labelset is token usage: operation name, provider, model, token
/// type, posture.
pub const MAX_LABELS: usize = 5;
/// Longest live key is `agentsfleet.billing.charge.type` (31 bytes).
pub const MAX_LABEL_KEY: usize = 32;
pub const MAX_LABEL_VAL: usize = 64;

pub const MetricId = enum {
    invoke_agent_duration,
    token_usage,
    cache_read_token_usage,
    credit_consumed,
    samples_dropped,
};

pub const MetricKind = enum { sum, histogram };

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
    /// Sum delta, or the observed value for a histogram. Always >= 0.
    value: i64,
    labels: [MAX_LABELS]Label,
    label_count: u8,
};

/// An aggregated series for one flush window: all same-`(id, labels)` samples
/// coalesced. Sums use `sum_value`; histograms use `hist_*` + `bucket_counts`.
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

/// A bare unsigned integer — the JSON number form for a count-unit histogram
/// bound and for its sum.
const FMT_UNSIGNED = "{d}";

pub const MetricMeta = struct {
    name: []const u8,
    unit: []const u8,
    kind: MetricKind,
    monotonic: bool,
    /// Explicit bucket bounds in the metric's *observation* unit; empty for sums.
    bounds: []const u64,
    /// Observations arrive in milliseconds but the metric declares seconds, so
    /// bounds and the histogram sum divide by `MILLIS_PER_SECOND` on the wire.
    millis_to_seconds: bool,
};

pub fn metaFor(id: MetricId) MetricMeta {
    return switch (id) {
        .invoke_agent_duration => .{
            .name = semconv.METRIC_INVOKE_AGENT_DURATION,
            .unit = semconv.UNIT_SECONDS,
            .kind = .histogram,
            .monotonic = false,
            .bounds = &semconv.DURATION_BUCKET_BOUNDS_MS,
            .millis_to_seconds = true,
        },
        .token_usage => .{
            .name = semconv.METRIC_INVOKE_AGENT_TOKEN_USAGE,
            .unit = semconv.UNIT_TOKENS,
            .kind = .histogram,
            .monotonic = false,
            .bounds = &semconv.TOKEN_BUCKET_BOUNDS,
            .millis_to_seconds = false,
        },
        .cache_read_token_usage => .{
            .name = semconv.METRIC_INVOKE_AGENT_CACHE_READ,
            .unit = semconv.UNIT_TOKENS,
            .kind = .histogram,
            .monotonic = false,
            .bounds = &semconv.TOKEN_BUCKET_BOUNDS,
            .millis_to_seconds = false,
        },
        .credit_consumed => .{
            .name = semconv.METRIC_BILLING_CREDIT_CONSUMED,
            .unit = semconv.UNIT_NANOCREDITS,
            .kind = .sum,
            .monotonic = true,
            .bounds = &.{},
            .millis_to_seconds = false,
        },
        .samples_dropped => .{
            .name = semconv.METRIC_SAMPLES_DROPPED,
            .unit = semconv.UNIT_COUNT,
            .kind = .sum,
            .monotonic = true,
            .bounds = &.{},
            .millis_to_seconds = false,
        },
    };
}

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

/// Print a millisecond quantity as the seconds JSON number the `s` unit
/// declares — integer part, then exactly three fractional digits. No float
/// arithmetic: a bound of 10 ms serializes as `0.010`, 81920 ms as `81.920`.
fn printSeconds(list: *std.ArrayList(u8), alloc: std.mem.Allocator, millis: u64) !void {
    try list.print(alloc, "{d}.{d:0>3}", .{ millis / semconv.MILLIS_PER_SECOND, millis % semconv.MILLIS_PER_SECOND });
}

fn appendSum(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, start_ns: u64, now_ns: u64) !void {
    try list.print(
        alloc,
        "\"sum\":{{\"aggregationTemporality\":{d},\"isMonotonic\":{s},\"dataPoints\":[{{",
        .{ AGGREGATION_TEMPORALITY_DELTA, if (meta.monotonic) "true" else "false" },
    );
    try appendAttributes(list, alloc, series.labels);
    try list.print(
        alloc,
        ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"asInt\":\"{d}\"}}]}}",
        .{ start_ns, now_ns, series.sum_value },
    );
}

fn appendHistogram(list: *std.ArrayList(u8), alloc: std.mem.Allocator, series: Series, meta: MetricMeta, start_ns: u64, now_ns: u64) !void {
    try list.print(alloc, "\"histogram\":{{\"aggregationTemporality\":{d},\"dataPoints\":[{{", .{AGGREGATION_TEMPORALITY_DELTA});
    try appendAttributes(list, alloc, series.labels);
    try list.print(alloc, ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"count\":\"{d}\",\"sum\":", .{ start_ns, now_ns, series.hist_count });
    const sum_magnitude: u64 = @intCast(@max(series.hist_sum, 0));
    if (meta.millis_to_seconds) {
        try printSeconds(list, alloc, sum_magnitude);
    } else {
        try list.print(alloc, FMT_UNSIGNED, .{sum_magnitude});
    }
    try list.appendSlice(alloc, ",\"bucketCounts\":[");
    for (series.bucket_counts[0 .. meta.bounds.len + 1], 0..) |bc, b| {
        if (b > 0) try list.appendSlice(alloc, ",");
        try list.print(alloc, "\"{d}\"", .{bc});
    }
    try list.appendSlice(alloc, "],\"explicitBounds\":[");
    for (meta.bounds, 0..) |bound, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        if (meta.millis_to_seconds) {
            try printSeconds(list, alloc, bound);
        } else {
            try list.print(alloc, FMT_UNSIGNED, .{bound});
        }
    }
    try list.appendSlice(alloc, "]}]}");
}

/// Serialize one aggregated series as a complete OTLP `metric` JSON object,
/// appended to `list`. `start_ns`/`now_ns` are the flush window bounds (delta
/// temporality). Caller writes the inter-object comma.
pub fn appendSeriesMetric(
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    series: Series,
    start_ns: u64,
    now_ns: u64,
) !void {
    const meta = metaFor(series.id);
    try list.print(alloc, "{{\"name\":\"{s}\",\"unit\":\"{s}\",", .{ meta.name, meta.unit });
    switch (meta.kind) {
        .sum => try appendSum(list, alloc, series, meta, start_ns, now_ns),
        .histogram => try appendHistogram(list, alloc, series, meta, start_ns, now_ns),
    }
    try list.appendSlice(alloc, "}");
}

/// Serialize aggregated series into one complete OTLP-JSON metrics envelope,
/// sharing the resource + scope serializer with the logs and traces signals.
pub fn serializeSeries(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    series: []const Series,
    start_ns: u64,
    now_ns: u64,
) ![]u8 {
    var list: std.ArrayList(u8) = .empty;
    errdefer list.deinit(alloc);
    try otlp_config.appendEnvelopePrefix(&list, alloc, cfg, "resourceMetrics", "scopeMetrics", "metrics");
    for (series, 0..) |s, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try appendSeriesMetric(&list, alloc, s, start_ns, now_ns);
    }
    try list.appendSlice(alloc, otlp_config.ENVELOPE_SUFFIX);
    return list.toOwnedSlice(alloc);
}
