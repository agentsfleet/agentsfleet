//! OTLP JSON metric exporter for Grafana Cloud Mimir.
//! The metering service layer pushes samples (credit-drain sum, token sum,
//! run-latency histogram); the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/metrics on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate. DELTA temporality — a Grafana Cloud
//! OTel Collector (deltatocumulative) converts before Mimir; see
//! otel_metrics_payload.zig. §7 replaces the per-sample ring with a windowed-delta
//! aggregation registry.

const std = @import("std");
const clock = @import("common").clock;
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");
const payload = @import("otel_metrics_payload.zig");
const cardinality = @import("otel_metrics_cardinality.zig");

const OTLP_METRICS_PATH = "/v1/metrics";
const BUFFER_CAPACITY: usize = 1024;
const FLUSH_BATCH_SIZE: usize = 50;

const Sample = payload.Sample;

const RingT = otlp_ring.Ring(Sample, BUFFER_CAPACITY);
var g_ring: RingT = .{};

const Exporter = otlp_exporter.Exporter(.{
    .path = OTLP_METRICS_PATH,
    .scope = .otel_metrics,
    .collect = collectMetrics,
    .pending = metricsPending,
});

pub const install = Exporter.install;
pub const uninstall = Exporter.uninstall;
pub const isInstalled = Exporter.isInstalled;

fn currentNanos() u64 {
    return @intCast(clock.nowNanos());
}

// ---------------------------------------------------------------------------
// Record API — non-blocking, fire-and-forget. No-ops when not installed.
// Callers invoke these AFTER the money transaction commits.
// ---------------------------------------------------------------------------

/// Record a committed credit-drain delta (nanos) labelled by posture/model and,
/// when under the cardinality cap, workspace.
pub fn recordCreditDrain(drained_nanos: i64, posture: []const u8, model: []const u8, workspace: []const u8) void {
    if (!isInstalled()) return;
    if (drained_nanos == 0) return;
    var s = payload.newSample(.credit_drain, drained_nanos, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    if (workspace.len > 0 and cardinality.allowWorkspace(workspace)) {
        _ = payload.addLabel(&s, payload.LABEL_WORKSPACE, workspace);
    }
    _ = g_ring.push(s);
}

/// Record a token-throughput delta for one direction (input/cached/output).
pub fn recordTokens(count: i64, direction: []const u8, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.tokens, count, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_DIRECTION, direction);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    _ = g_ring.push(s);
}

/// Observe a run's wall-clock duration (ms) into the latency histogram.
pub fn observeRunDuration(wall_ms: i64, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.run_duration, wall_ms, currentNanos());
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    _ = g_ring.push(s);
}

/// Emit the full metric bundle for one terminal run settlement: the stage
/// credit drained (final slice), token throughput per direction (the run's
/// cumulative totals), and the run-latency observation. Called post-commit
/// from the service layer (`service_report`), never from the money modules.
pub fn recordRunSettlement(
    charged_nanos: i64,
    input_tokens: i64,
    cached_tokens: i64,
    output_tokens: i64,
    wall_ms: i64,
    posture: []const u8,
    model: []const u8,
    workspace: []const u8,
) void {
    if (!isInstalled()) return;
    recordCreditDrain(charged_nanos, posture, model, workspace);
    recordTokens(input_tokens, payload.DIRECTION_INPUT, posture, model);
    recordTokens(cached_tokens, payload.DIRECTION_CACHED, posture, model);
    recordTokens(output_tokens, payload.DIRECTION_OUTPUT, posture, model);
    observeRunDuration(wall_ms, posture, model);
}

// ---------------------------------------------------------------------------
// Serialization (the exporter's collect hook)
// ---------------------------------------------------------------------------

fn metricsPending() bool {
    return g_ring.len() > 0;
}

fn collectMetrics(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    var batch: [FLUSH_BATCH_SIZE]Sample = undefined;
    var n: usize = 0;
    while (n < FLUSH_BATCH_SIZE) {
        batch[n] = g_ring.pop() orelse break;
        n += 1;
    }
    if (n == 0) return null;
    return try payload.serializeBatch(alloc, cfg.service_name, batch[0..n]);
}

// ---------------------------------------------------------------------------
// Test hooks
// ---------------------------------------------------------------------------

/// Test hook: number of samples currently pending in the global ring.
pub fn testPendingCount() usize {
    return g_ring.len();
}

/// Test hook: mark installed WITHOUT spawning the flush thread (deterministic).
pub fn testSetInstalled(cfg: otlp_config.GrafanaOtlpConfig) void {
    Exporter.testSetInstalled(cfg);
}

/// Test hook: pop one sample from the global ring.
pub fn testPop() ?Sample {
    return g_ring.pop();
}

/// Test hook: reset installed state and drain the ring.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
}

pub const TestRing = RingT;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

test {
    _ = @import("otel_metrics_test.zig");
}
