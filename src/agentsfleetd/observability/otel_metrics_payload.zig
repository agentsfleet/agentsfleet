//! OTLP-JSON metric serialization for the Grafana Cloud Mimir exporter.
//! Holds the metric-name/unit/label UFS constants (the wire schema shared
//! with the Grafana dashboard JSON), the fixed-size `Sample` input type, the
//! aggregated `Series` type, and the per-series serializer the flush loop calls.
//!
//! Temporality is DELTA: the flush coalesces a window's samples into one
//! `Series` per (metric, labelset) — see otel_metrics_aggregate.zig — each
//! serialized as a single dataPoint. A Fly-deployed OTel Collector
//! (deltatocumulative) converts delta → cumulative before Mimir.

const std = @import("std");

// ---------------------------------------------------------------------------
// Wire schema — metric names / units / label keys (UFS named constants).
// Any name here that a Grafana panel references is the single source of truth.
// ---------------------------------------------------------------------------

pub const METRIC_CREDIT_DRAIN = "agentsfleet.credit.drained_nanos";
pub const METRIC_TOKENS = "agentsfleet.tokens.processed";
pub const METRIC_RUN_DURATION = "agentsfleet.run.duration_ms";
/// Self-observability: samples the exporter dropped (ring-full + series-cap).
pub const METRIC_SAMPLES_DROPPED = "agentsfleet.telemetry.samples_dropped";

const UNIT_NANOS = "ns";
const UNIT_TOKENS = "{token}";
const UNIT_MILLIS = "ms";
const UNIT_COUNT = "1";

pub const LABEL_POSTURE = "posture";
pub const LABEL_MODEL = "model";
pub const LABEL_WORKSPACE = "workspace";
pub const LABEL_DIRECTION = "direction";

pub const DIRECTION_INPUT = "input";
pub const DIRECTION_CACHED = "cached";
pub const DIRECTION_OUTPUT = "output";

/// OTLP AggregationTemporality enum: 1 = DELTA, 2 = CUMULATIVE.
const AGGREGATION_TEMPORALITY_DELTA: u8 = 1;

/// Run-latency histogram bucket upper bounds, in milliseconds. The serialized
/// `explicitBounds`; bucketCounts has length = bounds.len + 1 (the trailing
/// +Inf bucket).
pub const DURATION_BUCKET_BOUNDS_MS = [_]u64{ 5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000, 10000 };

// ---------------------------------------------------------------------------
// Fixed-size sample (no heap; copied by value into the ring like SpanEntry).
// ---------------------------------------------------------------------------

pub const MAX_LABELS: usize = 4;
pub const MAX_LABEL_KEY: usize = 16;
pub const MAX_LABEL_VAL: usize = 64;

pub const MetricId = enum { credit_drain, tokens, run_duration, samples_dropped };

/// Number of histogram buckets = explicit bounds + the trailing +Inf bucket.
pub const N_BUCKETS: usize = DURATION_BUCKET_BOUNDS_MS.len + 1;
pub const MetricKind = enum { sum, histogram };

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

const MetricMeta = struct {
    name: []const u8,
    unit: []const u8,
    kind: MetricKind,
    monotonic: bool,
};

pub fn metaFor(id: MetricId) MetricMeta {
    return switch (id) {
        .credit_drain => .{ .name = METRIC_CREDIT_DRAIN, .unit = UNIT_NANOS, .kind = .sum, .monotonic = true },
        .tokens => .{ .name = METRIC_TOKENS, .unit = UNIT_TOKENS, .kind = .sum, .monotonic = true },
        .run_duration => .{ .name = METRIC_RUN_DURATION, .unit = UNIT_MILLIS, .kind = .histogram, .monotonic = false },
        .samples_dropped => .{ .name = METRIC_SAMPLES_DROPPED, .unit = UNIT_COUNT, .kind = .sum, .monotonic = true },
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

/// Index of the histogram bucket a value falls in: first bound it is <=, else
/// the trailing +Inf bucket (== bounds.len).
pub fn bucketIndex(value_ms: u64) usize {
    for (DURATION_BUCKET_BOUNDS_MS, 0..) |bound, i| {
        if (value_ms <= bound) return i;
    }
    return DURATION_BUCKET_BOUNDS_MS.len;
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
        .sum => {
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
        },
        .histogram => {
            try list.print(
                alloc,
                "\"histogram\":{{\"aggregationTemporality\":{d},\"dataPoints\":[{{",
                .{AGGREGATION_TEMPORALITY_DELTA},
            );
            try appendAttributes(list, alloc, series.labels);
            try list.print(
                alloc,
                ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"count\":\"{d}\",\"sum\":{d},\"bucketCounts\":[",
                .{ start_ns, now_ns, series.hist_count, series.hist_sum },
            );
            for (series.bucket_counts, 0..) |bc, b| {
                if (b > 0) try list.appendSlice(alloc, ",");
                try list.print(alloc, "\"{d}\"", .{bc});
            }
            try list.appendSlice(alloc, "],\"explicitBounds\":[");
            for (DURATION_BUCKET_BOUNDS_MS, 0..) |bound, i| {
                if (i > 0) try list.appendSlice(alloc, ",");
                try list.print(alloc, "{d}", .{bound});
            }
            try list.appendSlice(alloc, "]}]}");
        },
    }

    try list.appendSlice(alloc, "}");
}

/// Serialize aggregated series into one complete OTLP-JSON metrics envelope.
pub fn serializeSeries(
    alloc: std.mem.Allocator,
    service_name: []const u8,
    series: []const Series,
    start_ns: u64,
    now_ns: u64,
) ![]u8 {
    var list: std.ArrayList(u8) = .empty;
    try list.print(
        alloc,
        "{{\"resourceMetrics\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":{f}}}}}]}},\"scopeMetrics\":[{{\"scope\":{{\"name\":\"agentsfleetd\"}},\"metrics\":[",
        .{std.json.fmt(service_name, .{})},
    );
    for (series, 0..) |s, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try appendSeriesMetric(&list, alloc, s, start_ns, now_ns);
    }
    try list.appendSlice(alloc, "]}]}]}");
    return list.toOwnedSlice(alloc);
}
