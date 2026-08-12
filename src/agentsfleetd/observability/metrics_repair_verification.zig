//! Bounded OpenTelemetry Protocol metrics for production-repair verification.
//!
//! Closed outcomes use generated fixed cells. The two latency observations
//! ride the bounded metrics ring; workspace, repository, commit, and event
//! identifiers stay in structured logs and can never grow metric cardinality.

const std = @import("std");
const instruments = @import("otel_instruments.zig");
const otel_metrics = @import("otel_metrics.zig");
const otlp_config = @import("otlp/config.zig");

const MILLISECONDS_PER_SECOND: i64 = 1_000;
const TEST_PRODUCTION_AT_MS: i64 = 1_000;
const TEST_QUEUE_AT_MS: i64 = 3_000;
const TEST_COMPLETED_AT_MS: i64 = 8_000;

pub const ProviderResult = enum { accepted, replayed, ignored_normalization, ignored_repository };
pub const Correlation = enum { matched, missed, ambiguous };
pub const EventOutcome = enum { emitted, replayed };
pub const VerifierOutcome = enum { queued, completed };

pub const PROVIDER_NAME = "agentsfleet_repair_provider_results_total";
pub const CORRELATION_NAME = "agentsfleet_repair_correlations_total";
pub const INTENTS_CREATED_NAME = "agentsfleet_repair_verification_intents_created_total";
pub const DISPATCH_RETRIED_NAME = "agentsfleet_repair_dispatch_retried_total";
pub const EVENT_NAME = "agentsfleet_repair_synthetic_events_total";
pub const VERIFIER_NAME = "agentsfleet_repair_verifier_runs_total";
pub const DUE_BATCH_NAME = "agentsfleet_repair_dispatch_due_batch";
pub const BACKLOG_AGE_NAME = "agentsfleet_repair_dispatch_oldest_age_seconds";
pub const PRODUCTION_TO_QUEUE_NAME = "agentsfleet_repair_production_to_queue_seconds";
pub const QUEUE_TO_COMPLETE_NAME = "agentsfleet_repair_queue_to_completion_seconds";

pub const HISTOGRAM_BOUNDS_MS = [_]u64{ 1_000, 5_000, 30_000, 300_000, 900_000, 960_000, 1_800_000, 3_600_000 };

pub fn incProviderResult(result: ProviderResult) void {
    instruments.inc(.repair_provider_results, .{ .outcome = result });
}

pub fn incCorrelation(result: Correlation) void {
    instruments.inc(.repair_correlations, .{ .outcome = result });
}

/// The dispatcher reads a bounded batch, so the gauge is a saturated sample:
/// the configured batch limit means "at least this many due intents".
pub fn observeDispatchDueBatch(pending: usize, oldest_age_ms: i64) void {
    instruments.set(.repair_dispatch_due_batch, .{}, @intCast(pending));
    const age_seconds: u64 = @intCast(@divFloor(@max(0, oldest_age_ms), MILLISECONDS_PER_SECOND));
    instruments.set(.repair_dispatch_oldest_age, .{}, age_seconds);
}

pub fn incIntentsCreated(n: usize) void {
    if (n > 0) instruments.add(.repair_verification_intents_created, .{}, @intCast(n));
}

pub fn incDispatchRetried() void {
    instruments.inc(.repair_dispatch_retried, .{});
}

pub fn observeEventQueued(replayed: bool, production_completed_at: i64, queued_at: i64) void {
    instruments.inc(.repair_synthetic_events, .{ .outcome = if (replayed) .replayed else .emitted });
    instruments.inc(.repair_verifier_runs, .{ .outcome = .queued });
    otel_metrics.observeRuntimeHistogram(.repair_production_to_queue, queued_at - production_completed_at);
}

pub fn observeVerifierCompleted(queued_at: i64, completed_at: i64) void {
    instruments.inc(.repair_verifier_runs, .{ .outcome = .completed });
    otel_metrics.observeRuntimeHistogram(.repair_queue_to_completion, completed_at - queued_at);
}

pub fn resetForTest() void {
    instruments.resetCellsForTest(&.{
        .repair_provider_results,
        .repair_correlations,
        .repair_verification_intents_created,
        .repair_dispatch_retried,
        .repair_synthetic_events,
        .repair_verifier_runs,
        .repair_dispatch_due_batch,
        .repair_dispatch_oldest_age,
    });
}

test "repair verification metrics keep bounded cells and evented latency" {
    const cfg: otlp_config.GrafanaOtlpConfig = .{
        .endpoint = "http://127.0.0.1:0",
        .instance_id = "repair-metrics-test",
        .api_key = "test-key",
        .service_version = "0.0.0-test",
    };
    otel_metrics.testSetInstalled(cfg);
    defer otel_metrics.testClear();
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

    try std.testing.expectEqual(@as(u64, 1), instruments.snapshotCell(.repair_provider_results, .{ .outcome = .accepted }));
    try std.testing.expectEqual(@as(u64, 1), instruments.snapshotCell(.repair_correlations, .{ .outcome = .ambiguous }));
    try std.testing.expectEqual(@as(u64, 2), instruments.snapshotCell(.repair_dispatch_due_batch, .{}));
    try std.testing.expectEqual(@as(u64, 2), instruments.snapshotCell(.repair_dispatch_oldest_age, .{}));
    try std.testing.expectEqual(@as(u64, 2), instruments.snapshotCell(.repair_verification_intents_created, .{}));
    try std.testing.expectEqual(@as(u64, 1), instruments.snapshotCell(.repair_synthetic_events, .{ .outcome = .emitted }));
    try std.testing.expectEqual(@as(u64, 1), instruments.snapshotCell(.repair_synthetic_events, .{ .outcome = .replayed }));

    const emitted = otel_metrics.testPop() orelse return error.ExpectedProductionLatency;
    const replayed = otel_metrics.testPop() orelse return error.ExpectedReplayLatency;
    const completed = otel_metrics.testPop() orelse return error.ExpectedCompletionLatency;
    try std.testing.expect(emitted.id == .repair_production_to_queue);
    try std.testing.expectEqual(@as(i64, 2_000), emitted.value);
    try std.testing.expectEqual(emitted.id, replayed.id);
    try std.testing.expectEqual(@as(i64, 2_000), replayed.value);
    try std.testing.expect(completed.id == .repair_queue_to_completion);
    try std.testing.expectEqual(@as(i64, 5_000), completed.value);
}
