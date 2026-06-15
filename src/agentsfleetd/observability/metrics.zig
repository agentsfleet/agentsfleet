//! In-process metrics registry exposed in Prometheus text format.

const std = @import("std");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_render.zig");

pub const incApiBackpressureRejections = mc.incApiBackpressureRejections;
pub const setApiInFlightRequests = mc.setApiInFlightRequests;
pub const incSseBackpressureRejections = mc.incSseBackpressureRejections;
pub const setSseInFlightStreams = mc.setSseInFlightStreams;
pub const incSseDroppedFrames = mc.incSseDroppedFrames;
pub const incSseHubReconnects = mc.incSseHubReconnects;
pub const snapshot = mc.snapshot;

pub const renderPrometheus = mr.renderPrometheus;

// Per-execution metrics left this process at the M80 cutover (execution moved
// to the runner). The old per-execution series + their re-exports were removed here;
// the runner emits its own engine metrics separately.

// Redis pool registration — Prometheus pull-side wiring.
const mrp = @import("metrics_redis_pool.zig");
pub const registerRedisPool = mrp.registerPool;
pub const clearRegisteredRedisPool = mrp.clearRegisteredPool;

test "prometheus render includes key live metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_api_in_flight_requests"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_triggered_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_worker_running 1"));
}

// Worker_running=false path; guards against the gauge always emitting 1.
test "prometheus render emits agent_worker_running 0 when worker is not running" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, false);
    defer alloc.free(body);
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_worker_running 0"));
}

// Regression lock for the removed reconciler. The standalone reconcile
// daemon and the side-effect outbox dead-letter counter were retired together;
// neither name should ever reappear in /metrics output without the supporting
// machinery being reintroduced first. Catches a re-export that ships a metric
// nothing increments (would silently flatline downstream dashboards).
test "prometheus render does not emit removed reconciler metrics" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_reconcile_running"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_side_effect_outbox_dead_letter_total"));
}

// Same lock for the flatlined series this purge removed: external retry/failure
// classification, run-limit reasons, the agent-duration histogram, per-agent
// completion/token counters, and per-workspace token series rendered while
// nothing incremented them.
test "prometheus render does not emit purged flatlined series" {
    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_external_retries_total"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_external_failures_total"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agentsfleetd_run_limit_exceeded_total"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_agent_duration_seconds"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_completed_total"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_tokens_total"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, body, 1, "agent_workspace_tokens_total"));
}

test "integration: api throughput guardrail metrics are exposed in prometheus output" {
    const alloc = std.testing.allocator;
    setApiInFlightRequests(3);
    incApiBackpressureRejections();
    setSseInFlightStreams(2);
    incSseBackpressureRejections();

    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_api_in_flight_requests 3"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_api_backpressure_rejections_total"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_sse_in_flight_streams 2"));
    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_sse_backpressure_rejections_total"));
}

// The triggered counter renders with its incremented value.
test "prometheus render includes the agent triggered counter after increment" {
    const metrics_agent = @import("metrics_agent.zig");
    metrics_agent.resetForTest();
    defer metrics_agent.resetForTest();

    metrics_agent.incAgentsTriggered();

    const alloc = std.testing.allocator;
    const body = try renderPrometheus(alloc, true);
    defer alloc.free(body);

    try std.testing.expect(std.mem.containsAtLeast(u8, body, 1, "agent_triggered_total 1"));
}

test {
    _ = @import("metrics_counters_test.zig");
    _ = @import("metrics_runner_test.zig");
    _ = @import("metrics_memory_test.zig");
}
