//! OpenTelemetry Protocol (OTLP) JSON metric exporter for Grafana Cloud Mimir.
//! The metering service layer pushes samples (credit-drain sum, token sum,
//! run-latency histogram); the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/metrics on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate. Delta temporality — a Grafana Cloud
//! OTel Collector (deltatocumulative) converts before Mimir; see
//! otel_metrics_payload.zig. Flush coalesces the window's samples into one
//! windowed-delta series per (metric, labelset) — see otel_metrics_aggregate.zig.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const health = @import("metrics_otel.zig");
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const cardinality = @import("otel_metrics_cardinality.zig");

const OTLP_METRICS_PATH = "/v1/metrics";
const BUFFER_CAPACITY: usize = 1024;

const Sample = payload.Sample;

const RingT = otlp_ring.Ring(Sample, BUFFER_CAPACITY);
var g_ring: RingT = .{};

// Flush-thread-owned window state (read/written only by the flush thread).
var g_window_start_ns: u64 = 0;
var g_last_ring_dropped: u64 = 0;

const Exporter = otlp_exporter.Exporter(.{
    .signal = .metrics,
    .path = OTLP_METRICS_PATH,
    .scope = .otel_metrics,
    .collect = collectMetrics,
    .pending_count = metricsPendingCount,
    .wake_threshold = 768,
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
    var s = payload.newSample(.credit_drain, drained_nanos);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    if (workspace.len > 0 and cardinality.allowWorkspace(workspace)) {
        _ = payload.addLabel(&s, payload.LABEL_WORKSPACE, workspace);
    }
    enqueueSample(s);
}

/// Record a token-throughput delta for one direction (input/cached/output).
pub fn recordTokens(count: i64, direction: []const u8, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.tokens, count);
    _ = payload.addLabel(&s, payload.LABEL_DIRECTION, direction);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    enqueueSample(s);
}

/// Observe a run's wall-clock duration (ms) into the latency histogram.
pub fn observeRunDuration(wall_ms: i64, posture: []const u8, model: []const u8) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.run_duration, wall_ms);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = payload.addLabel(&s, payload.LABEL_MODEL, model);
    enqueueSample(s);
}

fn enqueueSample(sample: Sample) void {
    if (g_ring.push(sample)) {
        health.setQueueDepth(.metrics, g_ring.len());
        Exporter.notifyAccepted();
    } else {
        health.recordDiscard(.metrics, .ring_full, 1);
    }
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

fn metricsPendingCount() usize {
    return g_ring.len();
}

fn collectMetrics(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    max_entries: usize,
) otlp_exporter.CollectResult {
    if (max_entries == 0) return .empty;
    const now = currentNanos();
    var agg = aggregate.Aggregator.init();
    const drained = drainMetrics(&agg, @min(max_entries, BUFFER_CAPACITY));
    const total_dropped = droppedSinceLastFlush(agg.dropped);
    health.recordDiscard(.metrics, .aggregate_cap, @intCast(agg.dropped));
    if (agg.count == 0 and total_dropped == 0) {
        g_window_start_ns = now;
        return .empty;
    }

    const start = if (g_window_start_ns == 0) now else g_window_start_ns;
    g_window_start_ns = now;
    const serialized = serializeMetrics(alloc, cfg, &agg, total_dropped, start, now) catch {
        return .{ .serialize_failed = drained };
    };
    return .{ .ready = .{
        .body = serialized.body,
        .removed_count = drained,
        .export_count = serialized.export_count,
    } };
}

fn drainMetrics(agg: *aggregate.Aggregator, limit: usize) usize {
    var drained: usize = 0;
    while (drained < limit) : (drained += 1) {
        const sample = g_ring.pop() orelse break;
        agg.add(sample);
    }
    return drained;
}

fn droppedSinceLastFlush(aggregate_dropped: u64) u64 {
    const ring_dropped_now = g_ring.droppedCount();
    const total = (ring_dropped_now - g_last_ring_dropped) + aggregate_dropped;
    g_last_ring_dropped = ring_dropped_now;
    return total;
}

const SerializedMetrics = struct {
    body: []const u8,
    export_count: usize,
};

fn serializeMetrics(
    alloc: std.mem.Allocator,
    cfg: otlp_config.GrafanaOtlpConfig,
    agg: *const aggregate.Aggregator,
    total_dropped: u64,
    start: u64,
    now: u64,
) !SerializedMetrics {
    var series_buf: [aggregate.MAX_SERIES + 1]payload.Series = undefined;
    const base = agg.toSeries(series_buf[0..aggregate.MAX_SERIES]);
    var count = base.len;
    if (total_dropped > 0) {
        series_buf[count] = .{
            .id = .samples_dropped,
            .labels = &[_]payload.Label{},
            .sum_value = @intCast(total_dropped),
            .hist_count = 0,
            .hist_sum = 0,
            .bucket_counts = &[_]u64{},
        };
        count += 1;
    }
    return .{
        .body = try payload.serializeSeries(alloc, cfg.service_name, series_buf[0..count], start, now),
        .export_count = count,
    };
}

// ---------------------------------------------------------------------------
// Test hooks
// ---------------------------------------------------------------------------

/// Test hook: number of samples currently pending in the global ring.
pub fn testPendingCount() usize {
    return metricsPendingCount();
}

/// Test hook: mark installed without spawning the flush thread.
pub fn testSetInstalled(cfg: otlp_config.GrafanaOtlpConfig) void {
    Exporter.testSetInstalled(common.globalIo(), cfg);
}

/// Test hook: pop one sample from the global ring.
pub fn testPop() ?Sample {
    const sample = g_ring.pop();
    health.setQueueDepth(.metrics, g_ring.len());
    return sample;
}

/// Test hook: reset installed state, drain the ring, reset window state.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
    g_window_start_ns = 0;
    g_last_ring_dropped = g_ring.droppedCount();
    health.setQueueDepth(.metrics, 0);
}

/// Test hook: run one flush collect (drains + aggregates the window).
pub fn testCollectOnce(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    return switch (collectMetrics(alloc, cfg, BUFFER_CAPACITY)) {
        .empty => null,
        .ready => |batch| batch.body,
        .serialize_failed => error.SerializationFailed,
    };
}

/// Test hook: accepted pushes counted toward the next exporter cycle.
pub fn testAcceptedSinceCycle() u32 {
    return Exporter.testAcceptedSinceCycle();
}

pub const TestRing = RingT;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

test {
    _ = @import("otel_metrics_test.zig");
}
