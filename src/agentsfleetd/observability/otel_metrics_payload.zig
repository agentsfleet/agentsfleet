//! The metric sample layout for the Grafana Cloud Mimir exporter: the
//! fixed-size `Sample` input type, the aggregated `Series` type, and the label
//! writers that assemble them. Rendering those types to OTLP JSON lives in
//! `otel_metrics_wire.zig`; family identity lives in
//! `otel_metrics_families.zig`. This file owns how a sample is built, not how
//! it is printed.
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

pub const MetricId = families.MetricId;

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

/// Attach a label whose value is a member of a declared closed enum. The value
/// cannot be refused — an unregistered one fails the build — so false means only
/// that the sample is full. That stays a runtime guard, not an assertion:
/// assertions vanish under ReleaseFast and a sixth label would write past the
/// fixed slot array.
pub fn addClosedLabel(sample: *Sample, comptime key: []const u8, value: anytype) bool {
    return addLabelAtIndex(sample, key, dims.valueIndexOf(@TypeOf(value), value));
}

/// Attach a closed label whose interned index the caller already holds — the
/// provider path, where normalization produced the ordinal.
pub fn addLabelAtIndex(sample: *Sample, comptime key: []const u8, val_idx: u16) bool {
    if (sample.label_count >= MAX_LABELS) return false;
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
