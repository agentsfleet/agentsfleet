//! OTLP JSON metric exporter for Grafana Cloud Mimir.
//! The metering service layer pushes samples (credit-drain sum, token sum,
//! run-latency histogram); the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/metrics on a background flush thread, fire-and-forget.
//!
//! Migrated onto the generic otlp/ substrate. DELTA temporality — a Grafana Cloud
//! OTel Collector (deltatocumulative) converts before Mimir; see
//! otel_metrics_payload.zig. Flush coalesces the window's samples into one
//! windowed-delta series per (metric, labelset) — see otel_metrics_aggregate.zig.

const std = @import("std");
const clock = @import("common").clock;
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");

const OTLP_METRICS_PATH = "/v1/metrics";
const BUFFER_CAPACITY: usize = 1024;

const Sample = payload.Sample;

const RingT = otlp_ring.Ring(Sample, BUFFER_CAPACITY);
var g_ring: RingT = .{};

// Flush-thread-owned window state (read/written only by the flush thread).
var g_window_start_ns: u64 = 0;
var g_last_ring_dropped: u64 = 0;

const Exporter = otlp_exporter.Exporter(.{
    .path = OTLP_METRICS_PATH,
    .scope = .otel_metrics,
    .collect = collectMetrics,
    .pending = metricsPending,
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

/// Record a committed credit-drain delta (nanos) with fixed posture labels.
pub fn recordCreditDrain(drained_nanos: i64, posture: []const u8) void {
    if (!isInstalled()) return;
    if (drained_nanos == 0) return;
    var s = payload.newSample(.credit_drain, drained_nanos);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = g_ring.push(s);
    Exporter.notify();
}

/// Record a token-throughput delta for one direction (input/cached/output).
pub fn recordTokens(count: i64, direction: []const u8, posture: []const u8) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.tokens, count);
    _ = payload.addLabel(&s, payload.LABEL_DIRECTION, direction);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = g_ring.push(s);
    Exporter.notify();
}

/// Observe a run's wall-clock duration (ms) into the latency histogram.
pub fn observeRunDuration(wall_ms: i64, posture: []const u8) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.run_duration, wall_ms);
    _ = payload.addLabel(&s, payload.LABEL_POSTURE, posture);
    _ = g_ring.push(s);
    Exporter.notify();
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
) void {
    if (!isInstalled()) return;
    recordCreditDrain(charged_nanos, posture);
    recordTokens(input_tokens, payload.DIRECTION_INPUT, posture);
    recordTokens(cached_tokens, payload.DIRECTION_CACHED, posture);
    recordTokens(output_tokens, payload.DIRECTION_OUTPUT, posture);
    observeRunDuration(wall_ms, posture);
}

// ---------------------------------------------------------------------------
// Serialization (the exporter's collect hook)
// ---------------------------------------------------------------------------

fn metricsPending() bool {
    return g_ring.len() > 0;
}

fn collectMetrics(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    const now = currentNanos();

    // Drain the window and coalesce same-(metric, labelset) samples into one
    // series each — 100 same-labelset samples become ONE dataPoint on the wire.
    // Bound the drain to one ring's worth of samples so producers refilling
    // mid-drain can't spin this loop unboundedly; the overflow waits for the
    // next flush (and counts as a ring drop if the ring fills).
    var agg = aggregate.Aggregator.init();
    var drained: usize = 0;
    while (drained < BUFFER_CAPACITY) : (drained += 1) {
        const s = g_ring.pop() orelse break;
        agg.add(s);
    }

    // Dropped delta since last flush: ring-full drops (a cumulative counter) plus
    // this window's series-cap drops.
    const ring_dropped_now = g_ring.droppedCount();
    const total_dropped = (ring_dropped_now - g_last_ring_dropped) + agg.dropped;
    g_last_ring_dropped = ring_dropped_now;

    if (agg.count == 0 and total_dropped == 0) {
        g_window_start_ns = now;
        return null;
    }

    const start = if (g_window_start_ns == 0) now else g_window_start_ns;
    // Advance the window BEFORE serialization so a serialize error can't strand
    // g_window_start_ns at a stale boundary (the drained samples are already gone).
    g_window_start_ns = now;
    var series_buf: [aggregate.MAX_SERIES + 1]payload.Series = undefined;
    const base = agg.toSeries(series_buf[0..aggregate.MAX_SERIES]);
    var count = base.len;
    if (total_dropped > 0) {
        // Self-observability: the exporter's own drop count, as a delta sum.
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

    return try payload.serializeSeries(alloc, cfg.service_name, series_buf[0..count], start, now);
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

/// Test hook: reset installed state, drain the ring, reset window state.
pub fn testClear() void {
    Exporter.testClear();
    while (g_ring.pop()) |_| {}
    g_window_start_ns = 0;
    g_last_ring_dropped = g_ring.droppedCount();
}

/// Test hook: run one flush collect (drains + aggregates the window).
pub fn testCollectOnce(alloc: std.mem.Allocator, cfg: otlp_config.GrafanaOtlpConfig) !?[]const u8 {
    return collectMetrics(alloc, cfg);
}

pub const TestRing = RingT;
pub const TEST_BUFFER_CAPACITY = BUFFER_CAPACITY;

test {
    _ = @import("otel_metrics_test.zig");
}
