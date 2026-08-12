//! OpenTelemetry Protocol (OTLP) JSON metric exporter for Grafana Cloud Mimir.
//! The metering service layer pushes samples (credit-drain sum, token sum,
//! run-latency histogram); the shared otlp.Exporter batches and POSTs to
//! GRAFANA_OTLP_ENDPOINT/v1/metrics on a background flush thread, fire-and-forget.
//!
//! Evented families are delta — a collector converts before Mimir. Each flush
//! coalesces them per (metric, labelset) via otel_metrics_aggregate.zig, then
//! folds in the runtime families (otel_metrics_runtime.zig): the sole egress.

const std = @import("std");
const common = @import("common");
const clock = common.clock;
const health = @import("metrics_otel.zig");
const otlp_config = @import("otlp/config.zig");
const otlp_ring = @import("otlp/ring.zig");
const otlp_exporter = @import("otlp/exporter.zig");
const payload = @import("otel_metrics_payload.zig");
const wire = @import("otel_metrics_wire.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const cardinality = @import("otel_metrics_cardinality.zig");
const runtime = @import("otel_metrics_runtime.zig");
const semconv = @import("semconv.zig");
const dims = @import("otel_metrics_dims.zig");
const Mode = @import("../state/tenant_provider.zig").Mode;

const OTLP_METRICS_PATH = "/v1/metrics";
const BUFFER_CAPACITY: usize = 1024;

const Sample = payload.Sample;

const RingT = otlp_ring.Ring(Sample, BUFFER_CAPACITY);
var g_ring: RingT = .{};

// Flush-thread-owned window state (read/written only by the flush thread).
var g_window_start_ns: u64 = 0;
var g_last_ring_dropped: u64 = 0;
/// Stamped on the first collect; the startTimeUnixNano every cumulative sum
/// carries, so a restart is visible as a new start time.
var g_process_start_ns: u64 = 0;

const Exporter = otlp_exporter.Exporter(.{
    .signal = .metrics,
    .path = OTLP_METRICS_PATH,
    .scope = .otel_metrics,
    .collect = collectMetrics,
    .pending_count = metricsPendingCount,
    .wake_threshold = 768,
    // Level and cumulative families are pending work even when no evented sample
    // is: an idle daemon must keep pushing, or dashboards go stale and store-side
    // absence stops meaning "exporter dead".
    .always_collect = true,
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

/// The identity dimensions every run metric shares. `provider` and `model` are
/// the raw stored values; this module decides whether either can be exported
/// as a standard attribute. Workspace and tenant are deliberately absent — they
/// never reach a metric.
pub const Attribution = struct {
    /// Closed by type. Parsed at the Postgres boundary, where billing also
    /// resolves it, so the metric reports the posture the system charged against.
    posture: Mode,
    provider: []const u8,
    model: []const u8,
};

/// Attach `gen_ai.provider.name` and `gen_ai.request.model` when each can be
/// represented safely, counting every omission. An unrepresentable value is
/// dropped, never truncated or coerced: a truncated model reads as a different
/// model, and a private spelling under a standard key claims interoperability
/// it does not have. The budget keys on the identity the wire carries — since
/// normalization folds case, two spellings of one provider export as a single
/// series and must not spend two attribution slots.
fn appendProviderAndModel(sample: *Sample, attr: Attribution) void {
    var keyed = attr.provider;
    if (semconv.providerOrdinal(attr.provider)) |ordinal| {
        _ = payload.addLabelAtIndex(sample, semconv.ATTR_PROVIDER_NAME, dims.providerValueIndex(ordinal));
        keyed = semconv.WELL_KNOWN_PROVIDERS[ordinal];
    } else {
        health.recordAttributeOmission(.provider_name, .unmapped_provider);
    }
    if (attr.model.len == 0) return;
    if (!payload.valueFits(attr.model)) {
        health.recordAttributeOmission(.request_model, .value_too_long);
        return;
    }
    if (!cardinality.admitModel(keyed, attr.model)) {
        health.recordAttributeOmission(.request_model, .budget_exhausted);
        return;
    }
    _ = payload.setDynamicLabel(sample, semconv.ATTR_REQUEST_MODEL, attr.model);
}

/// Record a committed credit debit (nanocredits) under its fixed charge class.
/// Every caller invokes this strictly after its money write commits, so a
/// rolled-back, fenced, or replayed operation contributes nothing.
pub fn recordCreditConsumed(nanos: i64, charge: semconv.ChargeClass, attr: Attribution) void {
    if (!isInstalled()) return;
    if (nanos == 0) return;
    var s = payload.newSample(.credit_consumed, nanos);
    _ = payload.addClosedLabel(&s, semconv.ATTR_CHARGE_TYPE, charge);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, attr.posture);
    appendProviderAndModel(&s, attr);
    enqueueSample(s);
}

/// Observe one direction of an invocation's aggregate token usage. Input
/// already includes cached input; cached detail is reported by
/// `observeCacheReadTokens` as a subset, never as a third additive direction.
pub fn observeTokenUsage(count: i64, token_type: semconv.TokenType, attr: Attribution) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.token_usage, count);
    _ = payload.addInternedLabel(&s, semconv.ATTR_OPERATION_NAME, semconv.OPERATION_INVOKE_AGENT);
    _ = payload.addClosedLabel(&s, semconv.ATTR_TOKEN_TYPE, token_type);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, attr.posture);
    appendProviderAndModel(&s, attr);
    enqueueSample(s);
}

/// Observe the cached-input subset of the input direction above.
pub fn observeCacheReadTokens(count: i64, attr: Attribution) void {
    if (!isInstalled()) return;
    if (count == 0) return;
    var s = payload.newSample(.cache_read_token_usage, count);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, attr.posture);
    appendProviderAndModel(&s, attr);
    enqueueSample(s);
}

/// Observe one agent invocation's wall-clock duration (milliseconds in, seconds
/// on the wire). `error_type` is null on a clean run and the coarse failure
/// verdict otherwise.
pub fn observeInvokeAgentDuration(wall_ms: i64, error_type: ?semconv.ErrorType, attr: Attribution) void {
    if (!isInstalled()) return;
    var s = payload.newSample(.invoke_agent_duration, wall_ms);
    _ = payload.addClosedLabel(&s, semconv.ATTR_EXECUTION_POSTURE, attr.posture);
    if (error_type) |value| _ = payload.addClosedLabel(&s, semconv.ATTR_ERROR_TYPE, value);
    appendProviderAndModel(&s, attr);
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

/// Emit the full metric bundle for one terminal run settlement: the final
/// committed credit slice, the invocation's aggregate token usage, and the
/// duration observation. Called post-commit from the service layer
/// (`service_report`), never from the money modules.
///
/// `input_tokens` and `cached_tokens` arrive as disjoint counts (the metering
/// CTE prices them at different rates), so the exported input direction is
/// their sum and the cached count is additionally reported as a subset.
pub fn recordRunSettlement(
    charged_nanos: i64,
    input_tokens: i64,
    cached_tokens: i64,
    output_tokens: i64,
    wall_ms: i64,
    error_type: ?semconv.ErrorType,
    attr: Attribution,
) void {
    if (!isInstalled()) return;
    recordCreditConsumed(charged_nanos, .settle, attr);
    observeTokenUsage(input_tokens + cached_tokens, .input, attr);
    observeTokenUsage(output_tokens, .output, attr);
    observeCacheReadTokens(cached_tokens, attr);
    observeInvokeAgentDuration(wall_ms, error_type, attr);
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
    if (g_process_start_ns == 0) g_process_start_ns = now;
    var agg = aggregate.Aggregator.init();
    const drained = drainMetrics(&agg, @min(max_entries, BUFFER_CAPACITY));
    // Runtime families join the same window AFTER the evented drain: their
    // declared worst case is part of the derived series ceiling, so they can
    // never be the samples that overflow it.
    runtime.collect(&agg);
    // Close the attribution window with the sample window it governs. The
    // series ceiling the budget is derived from is per-flush (the Aggregator is
    // rebuilt every window), so holding admissions across windows would let
    // models that have gone idle keep slots they no longer spend, starving the
    // models actually running now while the window sits under its ceiling.
    cardinality.reset();
    const total_dropped = droppedSinceLastFlush(agg.dropped);
    health.recordDiscard(.metrics, .aggregate_cap, @intCast(agg.dropped));

    const start = if (g_window_start_ns == 0) now else g_window_start_ns;
    g_window_start_ns = now;
    const times = payload.WireTimes{
        .window_start_ns = start,
        .process_start_ns = g_process_start_ns,
        .now_ns = now,
    };
    const serialized = serializeMetrics(alloc, cfg, &agg, total_dropped, times) catch {
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
    times: payload.WireTimes,
) !SerializedMetrics {
    var series_buf: [aggregate.MAX_SERIES + 1]payload.Series = undefined;
    const base = agg.toSeries(series_buf[0..aggregate.MAX_SERIES]);
    var count = base.len;
    if (total_dropped > 0) {
        series_buf[count] = .{
            .id = .samples_dropped,
            .labels = &[_]payload.Label{},
            .dynamic = &.{},
            .sum_value = @intCast(total_dropped),
            .hist_count = 0,
            .hist_sum = 0,
            .bucket_counts = &[_]u64{},
        };
        count += 1;
    }
    const envelope = try wire.serializeSeries(alloc, cfg, series_buf[0..count], times, runtime.appendStreamedRunnerFamilies);
    // Streamed series shed at the payload budget are a real data loss the
    // operator must be able to see; appended ones join the export count the
    // backend's partial-rejection reply is validated against.
    if (envelope.extra.shed > 0) {
        health.recordDiscard(.metrics, .aggregate_cap, @intCast(envelope.extra.shed));
    }
    return .{
        .body = envelope.body,
        .export_count = count + envelope.extra.appended,
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

/// Test hook: enqueue a hand-built sample, bypassing attribution. Lets a test
/// exercise the aggregator's distinct-series cap with label sets the bounded
/// record API would never produce.
pub fn testPush(sample: Sample) void {
    enqueueSample(sample);
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
    g_process_start_ns = 0;
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
