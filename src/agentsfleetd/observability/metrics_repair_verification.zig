//! Fixed-cardinality Prometheus telemetry for production repair verification.
//!
//! All labels are bounded enums; workspace, repository, commit, and event
//! identifiers stay in structured logs so one malicious webhook cannot create
//! unbounded scrape-cardinality.

const std = @import("std");

const COUNTER = "counter";
const GAUGE = "gauge";
const HISTOGRAM = "histogram";
const FMT_HELP_TYPE = "# HELP {s} {s}\n# TYPE {s} {s}\n";
const FMT_VALUE = "{s} {d}\n";
const LABEL_OUTCOME = "outcome";
const MILLISECONDS_PER_SECOND: f64 = 1_000.0;
const TEST_PRODUCTION_AT_MS: i64 = 1_000;
const TEST_QUEUE_AT_MS: i64 = 3_000;
const TEST_COMPLETED_AT_MS: i64 = 8_000;

pub const ProviderResult = enum { accepted, replayed, ignored_normalization, ignored_repository };
pub const Correlation = enum { matched, missed, ambiguous };

const PROVIDER_NAME = "agentsfleet_repair_provider_results_total";
const CORRELATION_NAME = "agentsfleet_repair_correlations_total";
const INTENTS_CREATED_NAME = "agentsfleet_repair_verification_intents_created_total";
const DISPATCH_RETRIED_NAME = "agentsfleet_repair_dispatch_retried_total";
const EVENT_NAME = "agentsfleet_repair_synthetic_events_total";
const VERIFIER_NAME = "agentsfleet_repair_verifier_runs_total";
const DUE_BATCH_NAME = "agentsfleet_repair_dispatch_due_batch";
const BACKLOG_AGE_NAME = "agentsfleet_repair_dispatch_oldest_age_seconds";
const PRODUCTION_TO_QUEUE_NAME = "agentsfleet_repair_production_to_queue_seconds";
const QUEUE_TO_COMPLETE_NAME = "agentsfleet_repair_queue_to_completion_seconds";

const HISTOGRAM_BOUNDS_MS = [_]i64{ 1_000, 5_000, 30_000, 300_000, 900_000, 960_000, 1_800_000, 3_600_000 };

const Histogram = struct {
    buckets: [HISTOGRAM_BOUNDS_MS.len]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** HISTOGRAM_BOUNDS_MS.len,
    count: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),
    sum_ms: std.atomic.Value(u64) = std.atomic.Value(u64).init(0),

    fn observe(self: *Histogram, elapsed_ms: i64) void {
        const nonnegative: u64 = @intCast(@max(0, elapsed_ms));
        _ = self.count.fetchAdd(1, .monotonic);
        _ = self.sum_ms.fetchAdd(nonnegative, .monotonic);
        for (HISTOGRAM_BOUNDS_MS, 0..) |bound, index| {
            if (elapsed_ms <= bound) _ = self.buckets[index].fetchAdd(1, .monotonic);
        }
    }

    fn reset(self: *Histogram) void {
        for (&self.buckets) |*bucket| bucket.store(0, .release);
        self.count.store(0, .release);
        self.sum_ms.store(0, .release);
    }
};

var provider_results: [@typeInfo(ProviderResult).@"enum".fields.len]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** @typeInfo(ProviderResult).@"enum".fields.len;
var correlations: [@typeInfo(Correlation).@"enum".fields.len]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** @typeInfo(Correlation).@"enum".fields.len;
var intents_created_total = std.atomic.Value(u64).init(0);
var dispatch_retried_total = std.atomic.Value(u64).init(0);
var event_emitted_total = std.atomic.Value(u64).init(0);
var event_replayed_total = std.atomic.Value(u64).init(0);
var verifier_queued_total = std.atomic.Value(u64).init(0);
var verifier_completed_total = std.atomic.Value(u64).init(0);
var dispatch_due_batch = std.atomic.Value(i64).init(0);
var dispatch_oldest_age_ms = std.atomic.Value(i64).init(0);
var production_to_queue = Histogram{};
var queue_to_complete = Histogram{};

pub fn incProviderResult(result: ProviderResult) void {
    _ = provider_results[@intFromEnum(result)].fetchAdd(1, .monotonic);
}

pub fn incCorrelation(result: Correlation) void {
    _ = correlations[@intFromEnum(result)].fetchAdd(1, .monotonic);
}

/// The dispatcher reads a bounded batch, so the gauge is a saturated sample:
/// the configured batch limit means "at least this many due intents".
pub fn observeDispatchDueBatch(pending: usize, oldest_age_ms: i64) void {
    dispatch_due_batch.store(@intCast(pending), .monotonic);
    dispatch_oldest_age_ms.store(@max(0, oldest_age_ms), .monotonic);
}

pub fn incIntentsCreated(n: usize) void {
    if (n > 0) _ = intents_created_total.fetchAdd(@intCast(n), .monotonic);
}

pub fn incDispatchRetried() void {
    _ = dispatch_retried_total.fetchAdd(1, .monotonic);
}

pub fn observeEventQueued(replayed: bool, production_completed_at: i64, queued_at: i64) void {
    if (replayed) {
        _ = event_replayed_total.fetchAdd(1, .monotonic);
    } else {
        _ = event_emitted_total.fetchAdd(1, .monotonic);
    }
    _ = verifier_queued_total.fetchAdd(1, .monotonic);
    production_to_queue.observe(queued_at - production_completed_at);
}

pub fn observeVerifierCompleted(queued_at: i64, completed_at: i64) void {
    _ = verifier_completed_total.fetchAdd(1, .monotonic);
    queue_to_complete.observe(completed_at - queued_at);
}

fn renderEnumFamily(writer: anytype, name: []const u8, help: []const u8, label: []const u8, comptime E: type, values: []const std.atomic.Value(u64)) !void {
    try writer.print(FMT_HELP_TYPE, .{ name, help, name, COUNTER });
    inline for (@typeInfo(E).@"enum".fields, 0..) |field, index| {
        try writer.print("{s}{{{s}=\"{s}\"}} {d}\n", .{ name, label, field.name, values[index].load(.acquire) });
    }
}

fn renderHistogram(writer: anytype, name: []const u8, help: []const u8, histogram: *const Histogram) !void {
    try writer.print(FMT_HELP_TYPE, .{ name, help, name, HISTOGRAM });
    for (HISTOGRAM_BOUNDS_MS, 0..) |bound_ms, index| {
        const seconds = @as(f64, @floatFromInt(bound_ms)) / MILLISECONDS_PER_SECOND;
        try writer.print("{s}_bucket{{le=\"{d}\"}} {d}\n", .{ name, seconds, histogram.buckets[index].load(.acquire) });
    }
    const count = histogram.count.load(.acquire);
    const sum_seconds = @as(f64, @floatFromInt(histogram.sum_ms.load(.acquire))) / MILLISECONDS_PER_SECOND;
    try writer.print("{s}_bucket{{le=\"+Inf\"}} {d}\n{s}_sum {d}\n{s}_count {d}\n", .{ name, count, name, sum_seconds, name, count });
}

pub fn renderPrometheus(writer: anytype) !void {
    try renderEnumFamily(writer, PROVIDER_NAME, "Production deployment results by ingestion outcome.", LABEL_OUTCOME, ProviderResult, &provider_results);
    try renderEnumFamily(writer, CORRELATION_NAME, "Repair verification correlations by outcome.", LABEL_OUTCOME, Correlation, &correlations);
    try writer.print(FMT_HELP_TYPE ++ FMT_VALUE, .{ INTENTS_CREATED_NAME, "Durable verifier intents created by exact repair correlation.", INTENTS_CREATED_NAME, COUNTER, INTENTS_CREATED_NAME, intents_created_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE ++ FMT_VALUE, .{ DISPATCH_RETRIED_NAME, "Verifier dispatch attempts that failed and will retry.", DISPATCH_RETRIED_NAME, COUNTER, DISPATCH_RETRIED_NAME, dispatch_retried_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE, .{ EVENT_NAME, "Synthetic verifier events by durable dispatch outcome.", EVENT_NAME, COUNTER });
    try writer.print("{s}{{outcome=\"emitted\"}} {d}\n{s}{{outcome=\"replayed\"}} {d}\n", .{ EVENT_NAME, event_emitted_total.load(.acquire), EVENT_NAME, event_replayed_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE, .{ VERIFIER_NAME, "Verifier Fleet runs by lifecycle outcome.", VERIFIER_NAME, COUNTER });
    try writer.print("{s}{{outcome=\"queued\"}} {d}\n{s}{{outcome=\"completed\"}} {d}\n", .{ VERIFIER_NAME, verifier_queued_total.load(.acquire), VERIFIER_NAME, verifier_completed_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE ++ FMT_VALUE, .{ DUE_BATCH_NAME, "Current due verifier intent sample, capped at the dispatcher batch limit.", DUE_BATCH_NAME, GAUGE, DUE_BATCH_NAME, dispatch_due_batch.load(.acquire) });
    const oldest_age_seconds = @as(f64, @floatFromInt(dispatch_oldest_age_ms.load(.acquire))) / MILLISECONDS_PER_SECOND;
    try writer.print(FMT_HELP_TYPE ++ FMT_VALUE, .{ BACKLOG_AGE_NAME, "Age of the oldest due verifier intent in seconds.", BACKLOG_AGE_NAME, GAUGE, BACKLOG_AGE_NAME, oldest_age_seconds });
    try renderHistogram(writer, PRODUCTION_TO_QUEUE_NAME, "Seconds from provider production completion to verifier queueing.", &production_to_queue);
    try renderHistogram(writer, QUEUE_TO_COMPLETE_NAME, "Seconds from verifier queueing to completed Fleet report.", &queue_to_complete);
}

pub fn resetForTest() void {
    for (&provider_results) |*value| value.store(0, .release);
    for (&correlations) |*value| value.store(0, .release);
    intents_created_total.store(0, .release);
    dispatch_retried_total.store(0, .release);
    event_emitted_total.store(0, .release);
    event_replayed_total.store(0, .release);
    verifier_queued_total.store(0, .release);
    verifier_completed_total.store(0, .release);
    dispatch_due_batch.store(0, .release);
    dispatch_oldest_age_ms.store(0, .release);
    production_to_queue.reset();
    queue_to_complete.reset();
}

test "repair verification metrics render bounded outcomes, due sample, and latency" {
    resetForTest();
    incProviderResult(.accepted);
    incProviderResult(.ignored_normalization);
    incCorrelation(.matched);
    incCorrelation(.ambiguous);
    observeDispatchDueBatch(2, 2_000);
    incDispatchRetried();
    incIntentsCreated(2);
    observeEventQueued(false, TEST_PRODUCTION_AT_MS, TEST_QUEUE_AT_MS);
    observeEventQueued(true, TEST_PRODUCTION_AT_MS, TEST_QUEUE_AT_MS);
    observeVerifierCompleted(TEST_QUEUE_AT_MS, TEST_COMPLETED_AT_MS);
    var buffer: [8_192]u8 = undefined;
    var writer = std.Io.Writer.fixed(&buffer);
    try renderPrometheus(&writer);
    const body = writer.buffered();
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_provider_results_total{outcome=\"accepted\"} 1") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_correlations_total{outcome=\"ambiguous\"} 1") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_dispatch_due_batch 2") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_verification_intents_created_total 2") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_synthetic_events_total{outcome=\"emitted\"} 1") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_synthetic_events_total{outcome=\"replayed\"} 1") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_production_to_queue_seconds_sum 4") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_production_to_queue_seconds_count 2") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "agentsfleet_repair_queue_to_completion_seconds_sum 5") != null);
}
