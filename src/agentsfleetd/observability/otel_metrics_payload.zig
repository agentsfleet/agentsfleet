//! OTLP-JSON metric serialization for the Grafana Cloud Mimir exporter.
//! Holds the metric-name/unit/label UFS constants (the wire contract shared
//! with the Grafana dashboard JSON), the fixed-size `Sample` type, and the
//! per-sample serializer that `otel_metrics.zig`'s flush loop calls.
//!
//! Temporality is DELTA: every enqueued sample becomes one metric object with
//! a single dataPoint. No in-process cumulative registry — Grafana Cloud's
//! OTLP endpoint converts delta to cumulative. This mirrors the fire-and-forget,
//! no-aggregation shape of otel_traces.zig.

const std = @import("std");

// ---------------------------------------------------------------------------
// Wire contract — metric names / units / label keys (UFS named constants).
// Any name here that a Grafana panel references is the single source of truth.
// ---------------------------------------------------------------------------

pub const METRIC_CREDIT_DRAIN = "agentsfleet.credit.drained_nanos";
pub const METRIC_TOKENS = "agentsfleet.tokens.processed";
pub const METRIC_RUN_DURATION = "agentsfleet.run.duration_ms";

const UNIT_NANOS = "ns";
const UNIT_TOKENS = "{token}";
const UNIT_MILLIS = "ms";

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

pub const MetricId = enum { credit_drain, tokens, run_duration };
pub const MetricKind = enum { sum, histogram };

pub const Label = struct {
    key: [MAX_LABEL_KEY]u8,
    key_len: u8,
    val: [MAX_LABEL_VAL]u8,
    val_len: u8,
};

pub const Sample = struct {
    id: MetricId,
    /// Sum delta, or the observed value for a histogram. Always >= 0.
    value: i64,
    timestamp_ns: u64,
    labels: [MAX_LABELS]Label,
    label_count: u8,
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
    };
}

/// Initialize an empty sample for `id` stamped at `timestamp_ns`.
pub fn newSample(id: MetricId, value: i64, timestamp_ns: u64) Sample {
    return .{
        .id = id,
        .value = value,
        .timestamp_ns = timestamp_ns,
        // SAFETY: indices [0, label_count) are written by addLabel before any
        // reader (serialization) touches them; slots past label_count are never read.
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

fn appendAttributes(list: *std.ArrayList(u8), alloc: std.mem.Allocator, sample: Sample) !void {
    try list.appendSlice(alloc, "\"attributes\":[");
    var i: u8 = 0;
    while (i < sample.label_count) : (i += 1) {
        if (i > 0) try list.appendSlice(alloc, ",");
        const lbl = sample.labels[i];
        // json.fmt emits the surrounding quotes (and escapes the interior), so
        // the format string must NOT wrap {f} in its own quotes.
        try list.print(alloc, "{{\"key\":\"{s}\",\"value\":{{\"stringValue\":{f}}}}}", .{
            lbl.key[0..lbl.key_len],
            std.json.fmt(lbl.val[0..lbl.val_len], .{}),
        });
    }
    try list.appendSlice(alloc, "]");
}

/// Serialize one sample as a complete OTLP `metric` JSON object, appended to
/// `list`. Caller writes the inter-object comma (mirrors otel_traces).
pub fn appendSampleMetric(list: *std.ArrayList(u8), alloc: std.mem.Allocator, sample: Sample) !void {
    const meta = metaFor(sample.id);
    try list.print(alloc, "{{\"name\":\"{s}\",\"unit\":\"{s}\",", .{ meta.name, meta.unit });

    switch (meta.kind) {
        .sum => {
            try list.print(
                alloc,
                "\"sum\":{{\"aggregationTemporality\":{d},\"isMonotonic\":{s},\"dataPoints\":[{{",
                .{ AGGREGATION_TEMPORALITY_DELTA, if (meta.monotonic) "true" else "false" },
            );
            try appendAttributes(list, alloc, sample);
            try list.print(
                alloc,
                ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"asInt\":\"{d}\"}}]}}",
                .{ sample.timestamp_ns, sample.timestamp_ns, sample.value },
            );
        },
        .histogram => {
            const obs: u64 = if (sample.value < 0) 0 else @intCast(sample.value);
            const idx = bucketIndex(obs);
            try list.print(
                alloc,
                "\"histogram\":{{\"aggregationTemporality\":{d},\"dataPoints\":[{{",
                .{AGGREGATION_TEMPORALITY_DELTA},
            );
            try appendAttributes(list, alloc, sample);
            try list.print(
                alloc,
                ",\"startTimeUnixNano\":\"{d}\",\"timeUnixNano\":\"{d}\",\"count\":\"1\",\"sum\":{d},\"bucketCounts\":[",
                .{ sample.timestamp_ns, sample.timestamp_ns, obs },
            );
            var b: usize = 0;
            const n_buckets = DURATION_BUCKET_BOUNDS_MS.len + 1;
            while (b < n_buckets) : (b += 1) {
                if (b > 0) try list.appendSlice(alloc, ",");
                try list.appendSlice(alloc, if (b == idx) "\"1\"" else "\"0\"");
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

/// Serialize a batch of samples into one complete OTLP-JSON metrics envelope.
/// Used by the flush loop and pinned by the payload-shape fixture test.
pub fn serializeBatch(alloc: std.mem.Allocator, service_name: []const u8, samples: []const Sample) ![]u8 {
    var list: std.ArrayList(u8) = .empty;
    try list.print(
        alloc,
        "{{\"resourceMetrics\":[{{\"resource\":{{\"attributes\":[{{\"key\":\"service.name\",\"value\":{{\"stringValue\":\"{s}\"}}}}]}},\"scopeMetrics\":[{{\"scope\":{{\"name\":\"agentsfleetd\"}},\"metrics\":[",
        .{service_name},
    );
    for (samples, 0..) |sample, i| {
        if (i > 0) try list.appendSlice(alloc, ",");
        try appendSampleMetric(&list, alloc, sample);
    }
    try list.appendSlice(alloc, "]}]}]}");
    return list.toOwnedSlice(alloc);
}
