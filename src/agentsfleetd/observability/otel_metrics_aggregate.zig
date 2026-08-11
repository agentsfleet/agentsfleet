//! Flush-time metric aggregation: coalesce same-`(metric, labelset)` samples
//! drained from the ring into one OTLP series each (windowed-delta — each flush
//! aggregates only the samples since the last flush). A transient object the
//! metrics flush builds per window: no globals, no lock (the single flush thread
//! owns it), so 100 same-labelset samples become ONE dataPoint on the wire.
//!
//! Series lookup is a fixed open-addressed hash over the sample's identity
//! (family id + interned label indices + dynamic bytes) — constant-time per
//! sample, versus the retired linear scan whose cost grew with every distinct
//! series in the window.

const std = @import("std");
const payload = @import("otel_metrics_payload.zig");
const families = @import("otel_metrics_families.zig");

/// Distinct-series cap per flush window — derived from the family registry
/// (cost sub-budget + every declared fixed-label runtime family), never
/// hand-picked. Beyond this, samples for new label sets are dropped and
/// counted (surfaced as agentsfleet.telemetry.samples_dropped); the comptime
/// assertion in otel_metrics_families.zig fails the build before a declared
/// worst case can reach that path.
pub const MAX_SERIES: usize = families.MAX_SERIES;

/// Bucket count of the open-addressed identity index: the first power of two
/// holding at least 2× headroom over the series ceiling, so a probe always
/// reaches an empty bucket long before wrapping.
const TABLE_CAPACITY: usize = blk: {
    var capacity: usize = 1;
    while (capacity < MAX_SERIES * 2) capacity *= 2;
    break :blk capacity;
};

/// Empty-bucket sentinel; bucket entries are accumulator ordinals otherwise.
const BUCKET_EMPTY: u16 = std.math.maxInt(u16);

/// Fixed Wyhash seed: identities only ever compare within one process's flush
/// window, so the hash needs determinism, not keying.
const HASH_SEED: u64 = 0;

comptime {
    std.debug.assert(TABLE_CAPACITY >= MAX_SERIES * 2);
    // Every accumulator ordinal must be distinguishable from the sentinel.
    std.debug.assert(MAX_SERIES < BUCKET_EMPTY);
}

const Accumulator = struct {
    id: payload.MetricId,
    labels: [payload.MAX_LABELS]payload.Label,
    label_count: u8,
    dynamic: [payload.MAX_LABEL_VAL]u8,
    dynamic_len: u8,
    sum_value: i64,
    hist_count: u64,
    hist_sum: i64,
    bucket_counts: [payload.N_BUCKETS]u64,
};

fn matches(acc: *const Accumulator, sample: payload.Sample) bool {
    if (acc.id != sample.id or acc.label_count != sample.label_count) return false;
    for (acc.labels[0..acc.label_count], sample.labels[0..sample.label_count]) |a, b| {
        if (a.key_idx != b.key_idx or a.val_idx != b.val_idx) return false;
    }
    return std.mem.eql(u8, acc.dynamic[0..acc.dynamic_len], sample.dynamic[0..sample.dynamic_len]);
}

/// Hash of everything `matches` compares — the two must agree or a colliding
/// identity would probe past its own series.
fn identityHash(sample: payload.Sample) u64 {
    var h = std.hash.Wyhash.init(HASH_SEED);
    h.update(&[_]u8{@intFromEnum(sample.id)});
    for (sample.labels[0..sample.label_count]) |label| {
        const val_idx = label.val_idx;
        h.update(&[_]u8{label.key_idx});
        h.update(std.mem.asBytes(&val_idx));
    }
    h.update(sample.dynamic[0..sample.dynamic_len]);
    return h.final();
}

fn accumulate(acc: *Accumulator, sample: payload.Sample) void {
    const meta = families.metaFor(sample.id);
    switch (meta.kind) {
        .histogram => {
            // Clamp once: a negative observation (e.g. clock-skew wall_ms) buckets at
            // 0 AND adds 0 to the sum, so hist_sum can never disagree with the bucket
            // counts or go negative.
            const clamped: i64 = if (sample.value < 0) 0 else sample.value;
            acc.hist_count += 1;
            // Saturating add: a runner can report wall_ms that saturates to
            // maxInt(i64), and two such in one window would overflow a plain += and
            // trap in ReleaseSafe. Cap at maxInt instead — telemetry, not money.
            acc.hist_sum +|= clamped;
            acc.bucket_counts[payload.bucketIndex(@intCast(clamped), meta.bounds)] += 1;
        },
        .sum => acc.sum_value +|= sample.value,
        // A gauge is a level, not a running total: within one flush window the
        // newest observation wins, so folding by assignment is the whole rule.
        // Compile-time kind dispatch makes the additive path unreachable here.
        .gauge => acc.sum_value = sample.value,
    }
}

pub const Aggregator = struct {
    // SAFETY: only accs[0..count] are ever read; each is fully initialized in
    // add() before its bucket entry is published.
    accs: [MAX_SERIES]Accumulator = undefined,
    buckets: [TABLE_CAPACITY]u16 = [_]u16{BUCKET_EMPTY} ** TABLE_CAPACITY,
    count: usize = 0,
    dropped: u64 = 0,

    pub fn init() Aggregator {
        return .{};
    }

    /// Fold one sample into its series (creating it on first sight). A new label
    /// set beyond MAX_SERIES is dropped + counted, never silently merged.
    pub fn add(self: *Aggregator, sample: payload.Sample) void {
        const mask = TABLE_CAPACITY - 1;
        var bucket = identityHash(sample) & mask;
        // Bounded probe: the index keeps ≥2× headroom over the accumulator
        // cap, so an empty bucket is always reachable; the counter is the
        // can't-happen backstop against a full wrap.
        var probes: usize = 0;
        while (probes < TABLE_CAPACITY) : (probes += 1) {
            const slot = self.buckets[bucket];
            if (slot == BUCKET_EMPTY) {
                if (self.count >= MAX_SERIES) {
                    self.dropped += 1;
                    return;
                }
                const acc = &self.accs[self.count];
                acc.id = sample.id;
                acc.label_count = sample.label_count;
                for (sample.labels[0..sample.label_count], 0..) |label, i| acc.labels[i] = label;
                acc.dynamic_len = sample.dynamic_len;
                @memcpy(acc.dynamic[0..sample.dynamic_len], sample.dynamic[0..sample.dynamic_len]);
                acc.sum_value = 0;
                acc.hist_count = 0;
                acc.hist_sum = 0;
                acc.bucket_counts = [_]u64{0} ** payload.N_BUCKETS;
                self.buckets[bucket] = @intCast(self.count);
                self.count += 1;
                accumulate(acc, sample);
                return;
            }
            if (matches(&self.accs[slot], sample)) {
                accumulate(&self.accs[slot], sample);
                return;
            }
            bucket = (bucket + 1) & mask;
        }
        // Unreachable while the headroom assertion holds; drop, never trap.
        self.dropped += 1;
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
                .dynamic = acc.dynamic[0..acc.dynamic_len],
                .sum_value = acc.sum_value,
                .hist_count = acc.hist_count,
                .hist_sum = acc.hist_sum,
                .bucket_counts = acc.bucket_counts[0..],
            };
        }
        return buf[0..n];
    }
};

/// Test hook: the bucket a sample's identity initially probes. Lets the
/// collision test pick two identities that provably share a bucket instead of
/// hoping a random pair collides.
pub fn testIdentityBucket(sample: payload.Sample) usize {
    return identityHash(sample) & (TABLE_CAPACITY - 1);
}

test {
    _ = @import("otel_metrics_aggregate_test.zig");
}
