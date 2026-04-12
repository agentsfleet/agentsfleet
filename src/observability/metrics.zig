//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_render.zig");
const mw = @import("metrics_workspace.zig");
const em = @import("../executor/executor_metrics.zig");

pub const DurationBuckets = mc.DurationBuckets;
pub const HistogramSnapshot = mc.HistogramSnapshot;
pub const Snapshot = mc.Snapshot;

pub const incExternalRetry = mc.incExternalRetry;
pub const incExternalFailure = mc.incExternalFailure;
pub const incRetryAfterHintsApplied = mc.incRetryAfterHintsApplied;
pub const addAgentTokens = mc.addAgentTokens;
pub const addBackoffWaitMs = mc.addBackoffWaitMs;
pub const incOutboxDeadLetter = mc.incOutboxDeadLetter;
pub const incApiBackpressureRejections = mc.incApiBackpressureRejections;
pub const setApiInFlightRequests = mc.setApiInFlightRequests;
pub const observeAgentDurationSeconds = mc.observeAgentDurationSeconds;
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

// Reconciler daemon gauge.
pub const setReconcileRunning = mc.setReconcileRunning;

pub const renderPrometheus = mr.renderPrometheus;

// M28_001 §2.0: Per-workspace metrics.
pub const wsAddTokens = mw.addTokens;
pub const wsIncGateRepairLoops = mw.incGateRepairLoops;
pub const wsRenderPrometheus = mw.renderPrometheus;

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

test "prometheus render includes key live metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_retries_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_failures_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_agent_duration_seconds_bucket"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_triggered_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_running 1"));
}

// T3 — worker_running=false path; guards against the gauge always emitting 1.
test "prometheus render emits zombie_worker_running 0 when worker is not running" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, false);
    defer alloc.free(body);
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_worker_running 0"));
}

test "integration: outbox dead-letter metric is exposed in prometheus output" {
    const alloc = std.testing.allocator;
    incOutboxDeadLetter();

    const body = try renderPrometheus(alloc, false);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
}

test "integration: api throughput guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setApiInFlightRequests(3);
    incApiBackpressureRejections();

    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
}

test "integration: otel exporter metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    incOtelExportTotal();
    incOtelExportFailed();
    setOtelLastSuccessAtMs(1710000000000);

    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_export_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_export_failed_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_otel_last_success_at_ms"));
}

test {
    _ = @import("metrics_counters_test.zig");
}
