//! Facade over the in-process runtime metric counters.
//!
//! Counter state lives in the sibling metrics_* modules; every family leaves
//! the process over the OTLP push pipeline (`otel_metrics_families.zig` is the
//! closed registry of exported families — the daemon's one metrics egress).

const mc = @import("metrics_counters.zig");
const mot = @import("metrics_otel.zig");

pub const incApiBackpressureRejections = mc.incApiBackpressureRejections;
pub const setApiInFlightRequests = mc.setApiInFlightRequests;
pub const incSseBackpressureRejections = mc.incSseBackpressureRejections;
pub const setSseInFlightStreams = mc.setSseInFlightStreams;
pub const incSseDroppedFrames = mc.incSseDroppedFrames;
pub const incSseHubReconnects = mc.incSseHubReconnects;
pub const snapshot = mc.snapshot;
pub const incTraceSuppressed = @import("metrics_trace.zig").inc;
pub const recordOtlpDiscard = mot.recordDiscard;
pub const setOtlpQueueDepth = mot.setQueueDepth;

// Redis pool registration — the OTLP collector reads pool statistics
// through this seam.
const mrp = @import("metrics_redis_pool.zig");
pub const registerRedisPool = mrp.registerPool;
pub const clearRegisteredRedisPool = mrp.clearRegisteredPool;

test {
    _ = @import("metrics_counters_test.zig");
    _ = @import("metrics_runner_test.zig");
    _ = @import("metrics_memory_test.zig");
    _ = @import("metrics_sensitive_memory_test.zig");
    _ = @import("metrics_otel_test.zig");
}
