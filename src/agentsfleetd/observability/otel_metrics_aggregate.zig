//! Flush-time metric aggregation: coalesce same-`(metric, labelset)` samples
//! drained from the ring into one OTLP series each (windowed-delta — each flush
//! aggregates only the samples since the last flush). A transient object the
//! metrics flush builds per window: no globals, no lock (the single flush thread
//! owns it), so 100 same-labelset samples become ONE dataPoint on the wire.

const std = @import("std");
const payload = @import("otel_metrics_payload.zig");

/// Distinct-series cap per flush window. Beyond this, samples for new label sets
/// are dropped and counted (surfaced as agentsfleet.telemetry.samples_dropped).
pub const MAX_SERIES: usize = 256;

const Accumulator = struct {
    id: payload.MetricId,
    labels: [payload.MAX_LABELS]payload.Label,
    label_count: u8,
    sum_value: i64,
    hist_count: u64,
    hist_sum: i64,
    bucket_counts: [payload.N_BUCKETS]u64,
};

fn matches(acc: *const Accumulator, sample: payload.Sample) bool {
    if (acc.id != sample.id or acc.label_count != sample.label_count) return false;
    var i: u8 = 0;
    while (i < sample.label_count) : (i += 1) {
        const a = acc.labels[i];
        const b = sample.labels[i];
        if (a.key_len != b.key_len or a.val_len != b.val_len) return false;
        if (!std.mem.eql(u8, a.key[0..a.key_len], b.key[0..b.key_len])) return false;
        if (!std.mem.eql(u8, a.val[0..a.val_len], b.val[0..b.val_len])) return false;
    }
    return true;
}

fn accumulate(acc: *Accumulator, sample: payload.Sample) void {
    if (payload.metaFor(sample.id).kind == .histogram) {
        // Clamp once: a negative observation (e.g. clock-skew wall_ms) buckets at
        // 0 AND adds 0 to the sum, so hist_sum can never disagree with the bucket
        // counts or go negative.
        const clamped: i64 = if (sample.value < 0) 0 else sample.value;
        acc.hist_count += 1;
        // Saturating add: a runner can report wall_ms that saturates to
        // maxInt(i64), and two such in one window would overflow a plain += and
        // trap in ReleaseSafe. Cap at maxInt instead — telemetry, not money.
        acc.hist_sum +|= clamped;
        acc.bucket_counts[payload.bucketIndex(@intCast(clamped))] += 1;
    } else {
        acc.sum_value +|= sample.value;
    }
}

pub const Aggregator = struct {
    // SAFETY: only accs[0..count] are ever read; each is fully initialized in
    // add() before count is bumped.
    accs: [MAX_SERIES]Accumulator = undefined,
    count: usize = 0,
    dropped: u64 = 0,

    pub fn init() Aggregator {
        return .{};
    }

    /// Fold one sample into its series (creating it on first sight). A new label
    /// set beyond MAX_SERIES is dropped + counted, never silently merged.
    pub fn add(self: *Aggregator, sample: payload.Sample) void {
        var i: usize = 0;
        while (i < self.count) : (i += 1) {
            if (matches(&self.accs[i], sample)) {
                accumulate(&self.accs[i], sample);
                return;
            }
        }
        if (self.count >= MAX_SERIES) {
            self.dropped += 1;
            return;
        }
        const acc = &self.accs[self.count];
        acc.id = sample.id;
        acc.label_count = sample.label_count;
        var j: u8 = 0;
        while (j < sample.label_count) : (j += 1) acc.labels[j] = sample.labels[j];
        acc.sum_value = 0;
        acc.hist_count = 0;
        acc.hist_sum = 0;
        acc.bucket_counts = [_]u64{0} ** payload.N_BUCKETS;
        self.count += 1;
        accumulate(acc, sample);
    }

    /// View each accumulator as a payload.Series (slices reference this
    /// Aggregator — valid as long as it lives). Returns the filled prefix.
    pub fn toSeries(self: *const Aggregator, buf: []payload.Series) []payload.Series {
        var n: usize = 0;
        while (n < self.count and n < buf.len) : (n += 1) {
            const acc = &self.accs[n];
            buf[n] = .{
                .id = acc.id,
                .labels = acc.labels[0..acc.label_count],
                .sum_value = acc.sum_value,
                .hist_count = acc.hist_count,
                .hist_sum = acc.hist_sum,
                .bucket_counts = acc.bucket_counts[0..],
            };
        }
        return buf[0..n];
    }
};

test {
    _ = @import("otel_metrics_aggregate_test.zig");
}
