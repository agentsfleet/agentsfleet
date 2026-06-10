//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_render.zig");

pub const DurationBuckets = mc.DurationBuckets;
pub const HistogramSnapshot = mc.HistogramSnapshot;
pub const Snapshot = mc.Snapshot;

pub const incExternalRetry = mc.incExternalRetry;
pub const incExternalFailure = mc.incExternalFailure;
pub const incRetryAfterHintsApplied = mc.incRetryAfterHintsApplied;
pub const addAgentTokens = mc.addAgentTokens;
pub const addBackoffWaitMs = mc.addBackoffWaitMs;
pub const incApiBackpressureRejections = mc.incApiBackpressureRejections;
pub const setApiInFlightRequests = mc.setApiInFlightRequests;
pub const incSseBackpressureRejections = mc.incSseBackpressureRejections;
pub const setSseInFlightStreams = mc.setSseInFlightStreams;
pub const incSseDroppedFrames = mc.incSseDroppedFrames;
pub const incSseHubReconnects = mc.incSseHubReconnects;
pub const observeAgentDurationSeconds = mc.observeAgentDurationSeconds;
pub const incGateRepairExhausted = mc.incGateRepairExhausted;
// M17_001 §1.3
pub const incRunLimitTokenBudgetExceeded = mc.incRunLimitTokenBudgetExceeded;
pub const incRunLimitWallTimeExceeded = mc.incRunLimitWallTimeExceeded;
pub const incRunLimitRepairLoopsExhausted = mc.incRunLimitRepairLoopsExhausted;
pub const snapshot = mc.snapshot;

pub const renderPrometheus = mr.renderPrometheus;

// Per-(workspace, zombie) token counter.

// Per-execution metrics left this process at the M80 cutover (execution moved
// to the runner). The old per-execution series + their re-exports were removed here;
// the runner emits its own engine metrics separately.

// Redis pool registration — Prometheus pull-side wiring.
const mrp = @import("metrics_redis_pool.zig");
pub const registerRedisPool = mrp.registerPool;
pub const clearRegisteredRedisPool = mrp.clearRegisteredPool;
pub const redisPoolSnapshot = mrp.snapshot;

test "prometheus render includes key live metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_retries_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_external_failures_total"));
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

// T7 — regression lock for the removed reconciler. The standalone reconcile
// daemon and the side-effect outbox dead-letter counter were retired together;
// neither name should ever reappear in /metrics output without the supporting
// machinery being reintroduced first. Catches a re-export that ships a metric
// nothing increments (would silently flatline downstream dashboards).
test "prometheus render does not emit removed reconciler metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "zombie_reconcile_running"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "zombie_side_effect_outbox_dead_letter_total"));
}

test "integration: api throughput guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setApiInFlightRequests(3);
    incApiBackpressureRejections();
    setSseInFlightStreams(2);
    incSseBackpressureRejections();

    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_in_flight_requests 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_sse_in_flight_streams 2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_sse_backpressure_rejections_total"));
}

// All zombie counters + histogram render in Prometheus output after increment.
test "T7: prometheus render includes all zombie counters and histogram after increment" {
    const metrics_zombie = @import("metrics_zombie.zig");
    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();

    metrics_zombie.incZombiesTriggered();
    metrics_zombie.incZombiesCompleted();
    metrics_zombie.incZombiesFailed();
    metrics_zombie.addZombieTokens(500);
    metrics_zombie.observeZombieExecutionSeconds(2_000);

    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_triggered_total 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_completed_total 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_failed_total 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_tokens_total 500"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_execution_seconds_count 1"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_execution_seconds_sum 2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "zombie_execution_seconds_bucket"));
}

test {
    _ = @import("metrics_counters_test.zig");
    _ = @import("metrics_runner_test.zig");
}
