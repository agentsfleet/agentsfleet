//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const scoring = @import("../pipeline/scoring.zig");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_render.zig");
const em = @import("../executor/executor_metrics.zig");

pub const DurationBuckets = mc.DurationBuckets;
pub const HistogramSnapshot = mc.HistogramSnapshot;
pub const Snapshot = mc.Snapshot;

pub const incRunsCreated = mc.incRunsCreated;
pub const incRunsCompleted = mc.incRunsCompleted;
pub const incRunsBlocked = mc.incRunsBlocked;
pub const incRunRetries = mc.incRunRetries;
pub const incExternalRetry = mc.incExternalRetry;
pub const incExternalFailure = mc.incExternalFailure;
pub const incRetryAfterHintsApplied = mc.incRetryAfterHintsApplied;
pub const incWorkerErrors = mc.incWorkerErrors;
pub const incSandboxShellRuns = mc.incSandboxShellRuns;
pub const incSandboxHostRuns = mc.incSandboxHostRuns;
pub const incSandboxBubblewrapRuns = mc.incSandboxBubblewrapRuns;
pub const incSandboxKillSwitches = mc.incSandboxKillSwitches;
pub const incSandboxPreflightFailures = mc.incSandboxPreflightFailures;
pub const incAgentEchoCalls = mc.incAgentEchoCalls;
pub const incAgentScoutCalls = mc.incAgentScoutCalls;
pub const incAgentWardenCalls = mc.incAgentWardenCalls;
pub const addAgentTokens = mc.addAgentTokens;
pub const addAgentTokensByActor = mc.addAgentTokensByActor;
pub const addBackoffWaitMs = mc.addBackoffWaitMs;
pub const addRateLimitWaitMs = mc.addRateLimitWaitMs;
pub const incOutboxEnqueued = mc.incOutboxEnqueued;
pub const incOutboxDelivered = mc.incOutboxDelivered;
pub const incOutboxDeadLetter = mc.incOutboxDeadLetter;
pub const incApiBackpressureRejections = mc.incApiBackpressureRejections;
pub const setApiInFlightRequests = mc.setApiInFlightRequests;
pub const setWorkerInFlightRuns = mc.setWorkerInFlightRuns;
pub const incWorkerAllocatorLeaks = mc.incWorkerAllocatorLeaks;
pub const incAgentScoreComputed = mc.incAgentScoreComputed;
pub const incAgentScoringFailed = mc.incAgentScoringFailed;
pub const setAgentScoreLatest = mc.setAgentScoreLatest;
pub const observeAgentScoringDurationMs = mc.observeAgentScoringDurationMs;
pub const observeAgentDurationSeconds = mc.observeAgentDurationSeconds;
pub const observeRunTotalWallSeconds = mc.observeRunTotalWallSeconds;
pub const incGateRepairLoops = mc.incGateRepairLoops;
pub const incGateRepairExhausted = mc.incGateRepairExhausted;
// M17_001 §1.3
pub const incRunLimitTokenBudgetExceeded = mc.incRunLimitTokenBudgetExceeded;
pub const incRunLimitWallTimeExceeded = mc.incRunLimitWallTimeExceeded;
pub const incRunLimitRepairLoopsExhausted = mc.incRunLimitRepairLoopsExhausted;
pub const snapshot = mc.snapshot;

// OTel exporter metrics.
pub const incOtelExportTotal = mc.incOtelExportTotal;
pub const incOtelExportFailed = mc.incOtelExportFailed;
pub const setOtelLastSuccessAtMs = mc.setOtelLastSuccessAtMs;

// Orphan recovery metrics (M14_001).
pub const incOrphanRunsRecovered = mc.incOrphanRunsRecovered;
pub const incOrphanNoAgentProfile = mc.incOrphanNoAgentProfile;
pub const setReconcileRunning = mc.setReconcileRunning;

pub const renderPrometheus = mr.renderPrometheus;

// Executor metrics re-exports (§5.2).
pub const incExecutorSessionsCreated = em.incExecutorSessionsCreated;
pub const setExecutorSessionsActive = em.setExecutorSessionsActive;
pub const incExecutorFailures = em.incExecutorFailures;
pub const incExecutorOomKills = em.incExecutorOomKills;
pub const incExecutorTimeoutKills = em.incExecutorTimeoutKills;
pub const incExecutorLandlockDenials = em.incExecutorLandlockDenials;
pub const incExecutorResourceKills = em.incExecutorResourceKills;
pub const incExecutorLeaseExpired = em.incExecutorLeaseExpired;
pub const incExecutorCancellations = em.incExecutorCancellations;
pub const addExecutorCpuThrottledMs = em.addExecutorCpuThrottledMs;
pub const setExecutorMemoryPeakBytes = em.setExecutorMemoryPeakBytes;
pub const executorSnapshot = em.executorSnapshot;

test "prometheus render includes key metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true, 3, 1200);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_runs_created_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_running 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_queue_depth 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_retries_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_failures_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_rate_limit_wait_ms_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_enqueued_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_agent_score_computed_unranked_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_agent_duration_seconds_bucket"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_run_total_wall_seconds_bucket"));
}

test "integration: outbox metric counters are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    incOutboxEnqueued();
    incOutboxDelivered();
    incOutboxDeadLetter();

    const body = try renderPrometheus(alloc, false, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_enqueued_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_delivered_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
}

test "integration: worker guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setWorkerInFlightRuns(2);
    incWorkerAllocatorLeaks();

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_in_flight_runs 2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_allocator_leaks_total"));
}

test "integration: api throughput guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setApiInFlightRequests(3);
    incApiBackpressureRejections();

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
}

test "integration: otel exporter metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    incOtelExportTotal();
    incOtelExportFailed();
    setOtelLastSuccessAtMs(1710000000000);

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_export_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_export_failed_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_last_success_at_ms"));
}

test "scoring metrics remain low-cardinality without agent labels" {
    const alloc = std.testing.allocator;
    incAgentScoreComputed(scoring.Tier.gold);
    setAgentScoreLatest(87);

    const body = try renderPrometheus(alloc, true, 0, 0);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_agent_score_latest 87"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "zombie_agent_score_latest{"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_id="));
}

// T3 — worker_running=false path; guards against the gauge always emitting 1
test "prometheus render emits zombie_worker_running 0 when worker is not running" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, false, 0, 0);
    defer alloc.free(body);
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_running 0"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_queue_depth 0"));
}

test {
    _ = @import("metrics_counters_test.zig");
}
