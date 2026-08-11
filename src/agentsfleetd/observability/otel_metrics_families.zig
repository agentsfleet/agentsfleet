//! The closed metric-family registry for the OTLP exporter.
//!
//! Every family the daemon exports — evented cost families and flush-time
//! runtime families alike — is declared here, once. The wire serializer
//! (otel_metrics_payload.zig), the aggregator's series ceiling
//! (otel_metrics_aggregate.zig), the model-attribution budget
//! (otel_metrics_cardinality.zig), the runtime collector
//! (otel_metrics_runtime.zig), and the census/namespace guard tests all read
//! this table, so a family cannot exist on the wire without being declared —
//! and the ceiling arithmetic cannot drift from what is actually exported.
//!
//! Three collection shapes:
//!   - `cost = true`  — evented samples through the lock-free ring; their
//!     series budget is `COST_SERIES_BUDGET` and the model-attribution cap
//!     derives from it, so widening the runtime set can never shrink it.
//!   - `streamed = true` — pre-aggregated per-runner families serialized
//!     straight from the slot table; they never enter the Aggregator, and
//!     their worst case is bounded by that table's own capacity.
//!   - neither — fixed-label runtime families, snapshot once per flush window
//!     into the Aggregator; `max_series` is the comptime product of their
//!     closed label enums.

const std = @import("std");
const semconv = @import("semconv.zig");
const mot = @import("metrics_otel.zig");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_runner.zig");
const mm = @import("metrics_memory.zig");
const msm = @import("metrics_sensitive_memory.zig");
const mt = @import("metrics_trace.zig");
const ls = @import("library_stages.zig");

pub const MetricKind = enum { sum, histogram, gauge };

/// OTLP AggregationTemporality for sums: evented families export the window's
/// delta (a Fly-deployed collector converts to cumulative); snapshot counters
/// are natively cumulative and export as such, needing no per-flush memo.
pub const Temporality = enum { delta, cumulative };

/// Unit conversion applied at serialization. Observations stay integer in
/// their source unit; the printer emits the declared unit exactly (no float
/// arithmetic anywhere).
pub const Scale = enum { none, millis_to_seconds, nanos_to_seconds };

pub const MetricMeta = struct {
    name: []const u8,
    unit: []const u8,
    kind: MetricKind,
    monotonic: bool = false,
    temporality: Temporality = .delta,
    /// Explicit bucket bounds in the metric's *observation* unit; empty for
    /// sums and gauges.
    bounds: []const u64 = &.{},
    scale: Scale = .none,
    /// Worst-case distinct label sets this family can carry in one flush.
    max_series: usize = 1,
    /// Serialized from a pre-aggregated source; never enters the Aggregator.
    streamed: bool = false,
    /// Evented cost family riding the ring; budgeted by COST_SERIES_BUDGET.
    cost: bool = false,
};

pub const MetricId = enum {
    // Evented cost families (delta, through the ring).
    invoke_agent_duration,
    token_usage,
    cache_read_token_usage,
    credit_consumed,
    samples_dropped,
    // API / SSE guardrails + worker liveness.
    api_backpressure_rejections,
    api_in_flight_requests,
    sse_backpressure_rejections,
    sse_in_flight_streams,
    sse_dropped_frames,
    sse_hub_reconnects,
    worker_running,
    fleet_triggered,
    // Trace admission.
    http_trace_suppressed,
    // Exporter self-health.
    otlp_queue_depth,
    otlp_entries_discarded,
    otel_attribute_omitted,
    // Signup.
    signup_bootstrapped,
    signup_replayed,
    signup_failed,
    // Lease-poll cost + readiness index.
    lease_polls,
    lease_poll_candidates_scanned,
    lease_poll_db_roundtrips,
    fleet_ready_depth,
    fleet_ready_write_failures,
    // Background maintenance.
    runner_retention_swept,
    runner_retention_sweep_failures,
    account_teardown_unregister_failures,
    // Library read evidence.
    library_stage_duration,
    library_stage_observations,
    library_read_outcome,
    library_pool_result,
    library_cache_outcome,
    library_payload_bytes,
    library_results,
    // Redis request-path pool.
    redis_pool_active,
    redis_pool_idle,
    redis_pool_dials,
    redis_pool_overflow_dials,
    redis_pool_poisoned_connections,
    redis_pool_reconnects,
    redis_pool_forced_closes,
    redis_pool_acquire_timeouts,
    // Memory plane.
    memory_entries_captured,
    memory_push_failures,
    memory_hydration_window_entries,
    memory_hydration_dropped_entries,
    memory_hydration_dropped_bytes,
    memory_cap_evictions,
    memory_capture_truncated,
    memory_capture_skipped,
    memory_search_zero_hits,
    // Sensitive-memory hygiene + process memory.
    process_resident_memory_bytes,
    sensitive_request_erased_bytes,
    sensitive_response_erased_bytes,
    sensitive_response_write_failures,
    // Per-runner families (streamed from the slot table).
    runner_failures,
    runner_failures_overflow,
    runner_executions,
    runner_last_seen_seconds,
    runner_active_leases,
};

/// Cumulative monotonic count sum with a fixed number of label combinations.
fn cum(name: []const u8, max_series: usize) MetricMeta {
    return .{ .name = name, .unit = semconv.UNIT_COUNT, .kind = .sum, .monotonic = true, .temporality = .cumulative, .max_series = max_series };
}

fn gauge(name: []const u8, unit: []const u8, max_series: usize) MetricMeta {
    return .{ .name = name, .unit = unit, .kind = .gauge, .max_series = max_series };
}

fn streamedMeta(base: MetricMeta, worst_case: usize) MetricMeta {
    var meta = base;
    meta.streamed = true;
    meta.max_series = worst_case;
    return meta;
}

fn costMeta(base: MetricMeta) MetricMeta {
    var meta = base;
    meta.cost = true;
    return meta;
}

fn buildMeta(id: MetricId) MetricMeta {
    return switch (id) {
        .invoke_agent_duration => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_DURATION, .unit = semconv.UNIT_SECONDS, .kind = .histogram, .bounds = &semconv.DURATION_BUCKET_BOUNDS_MS, .scale = .millis_to_seconds }),
        .token_usage => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_TOKEN_USAGE, .unit = semconv.UNIT_TOKENS, .kind = .histogram, .bounds = &semconv.TOKEN_BUCKET_BOUNDS }),
        .cache_read_token_usage => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_CACHE_READ, .unit = semconv.UNIT_TOKENS, .kind = .histogram, .bounds = &semconv.TOKEN_BUCKET_BOUNDS }),
        .credit_consumed => costMeta(.{ .name = semconv.METRIC_BILLING_CREDIT_CONSUMED, .unit = semconv.UNIT_NANOCREDITS, .kind = .sum, .monotonic = true }),
        .samples_dropped => costMeta(.{ .name = semconv.METRIC_SAMPLES_DROPPED, .unit = semconv.UNIT_COUNT, .kind = .sum, .monotonic = true }),
        .api_backpressure_rejections => cum(semconv.METRIC_API_BACKPRESSURE_REJECTIONS, 1),
        .api_in_flight_requests => gauge(semconv.METRIC_API_IN_FLIGHT_REQUESTS, semconv.UNIT_REQUESTS, 1),
        .sse_backpressure_rejections => cum(semconv.METRIC_SSE_BACKPRESSURE_REJECTIONS, 1),
        .sse_in_flight_streams => gauge(semconv.METRIC_SSE_IN_FLIGHT_STREAMS, semconv.UNIT_STREAMS, 1),
        .sse_dropped_frames => cum(semconv.METRIC_SSE_DROPPED_FRAMES, 1),
        .sse_hub_reconnects => cum(semconv.METRIC_SSE_HUB_RECONNECTS, 1),
        .worker_running => gauge(semconv.METRIC_WORKER_RUNNING, semconv.UNIT_WORKERS, 1),
        .fleet_triggered => cum(semconv.METRIC_FLEET_TRIGGERED, 1),
        .http_trace_suppressed => cum(mt.SUPPRESSED_NAME, mt.SUPPRESSION_REASON_LABELS.len),
        .otlp_queue_depth => gauge(mot.QUEUE_DEPTH_NAME, semconv.UNIT_ENTRIES, mot.SIGNALS.len),
        .otlp_entries_discarded => cum(mot.DISCARDED_NAME, mot.SIGNALS.len * mot.DISCARD_REASONS.len),
        .otel_attribute_omitted => cum(mot.ATTRIBUTE_OMITTED_NAME, mot.OMITTED_ATTRIBUTES.len * mot.OMISSION_REASONS.len),
        .signup_bootstrapped => cum(semconv.METRIC_SIGNUP_BOOTSTRAPPED, 1),
        .signup_replayed => cum(semconv.METRIC_SIGNUP_REPLAYED, 1),
        .signup_failed => cum(semconv.METRIC_SIGNUP_FAILED, mc.SIGNUP_FAIL_REASON_LABELS.len),
        .lease_polls => cum(mc.LEASE_POLLS_NAME, 1),
        .lease_poll_candidates_scanned => cum(mc.CANDIDATES_SCANNED_NAME, 1),
        .lease_poll_db_roundtrips => cum(mc.DB_ROUNDTRIPS_NAME, 1),
        .fleet_ready_depth => gauge(mc.READY_DEPTH_NAME, semconv.UNIT_FLEETS, 1),
        .fleet_ready_write_failures => cum(mc.READY_WRITE_FAILURES_NAME, 1),
        .runner_retention_swept => cum(mc.RETENTION_SWEPT_NAME, 1),
        .runner_retention_sweep_failures => cum(mc.RETENTION_SWEEP_FAILURES_NAME, 1),
        .account_teardown_unregister_failures => cum(mc.TEARDOWN_UNREGISTER_FAILURES_NAME, 1),
        .library_stage_duration => .{ .name = ls.STAGE_DURATION_NAME, .unit = semconv.UNIT_SECONDS, .kind = .sum, .monotonic = true, .temporality = .cumulative, .scale = .nanos_to_seconds, .max_series = ls.SURFACE_LABELS.len * ls.STAGE_LABELS.len },
        .library_stage_observations => cum(ls.STAGE_OBSERVATIONS_NAME, ls.SURFACE_LABELS.len * ls.STAGE_LABELS.len),
        .library_read_outcome => cum(ls.READ_OUTCOME_NAME, ls.SURFACE_LABELS.len * ls.OUTCOME_LABELS.len),
        .library_pool_result => cum(ls.POOL_RESULT_NAME, ls.POOL_RESULT_LABELS.len),
        .library_cache_outcome => cum(ls.CACHE_OUTCOME_NAME, ls.CACHE_LABELS.len),
        .library_payload_bytes => .{ .name = ls.PAYLOAD_BYTES_NAME, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative, .max_series = ls.SURFACE_LABELS.len },
        .library_results => cum(ls.RESULTS_NAME, ls.SURFACE_LABELS.len),
        .redis_pool_active => gauge(semconv.METRIC_REDIS_POOL_ACTIVE, semconv.UNIT_CONNECTIONS, 1),
        .redis_pool_idle => gauge(semconv.METRIC_REDIS_POOL_IDLE, semconv.UNIT_CONNECTIONS, 1),
        .redis_pool_dials => cum(semconv.METRIC_REDIS_POOL_DIALS, 1),
        .redis_pool_overflow_dials => cum(semconv.METRIC_REDIS_POOL_OVERFLOW_DIALS, 1),
        .redis_pool_poisoned_connections => cum(semconv.METRIC_REDIS_POOL_POISONED, 1),
        .redis_pool_reconnects => cum(semconv.METRIC_REDIS_POOL_RECONNECTS, 1),
        .redis_pool_forced_closes => cum(semconv.METRIC_REDIS_POOL_FORCED_CLOSES, 1),
        .redis_pool_acquire_timeouts => cum(semconv.METRIC_REDIS_POOL_ACQUIRE_TIMEOUTS, 1),
        .memory_entries_captured => cum(mm.MEM_CAPTURED_NAME, 1),
        .memory_push_failures => cum(mm.MEM_PUSH_FAIL_NAME, 1),
        .memory_hydration_window_entries => gauge(mm.MEM_HYDRATION_NAME, semconv.UNIT_ENTRIES, 1),
        .memory_hydration_dropped_entries => cum(mm.HYDRATION_DROPPED_ENTRIES_NAME, 1),
        .memory_hydration_dropped_bytes => .{ .name = mm.HYDRATION_DROPPED_BYTES_NAME, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative },
        .memory_cap_evictions => cum(mm.CAP_EVICTIONS_NAME, 1),
        .memory_capture_truncated => cum(mm.CAPTURE_TRUNCATED_NAME, 1),
        .memory_capture_skipped => cum(mm.CAPTURE_SKIPPED_NAME, 1),
        .memory_search_zero_hits => cum(mm.SEARCH_ZERO_HITS_NAME, 1),
        .process_resident_memory_bytes => gauge(msm.METRIC_PROCESS_RESIDENT_MEMORY, semconv.UNIT_BYTES, 1),
        .sensitive_request_erased_bytes => .{ .name = msm.METRIC_REQUEST_ERASED_BYTES, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative },
        .sensitive_response_erased_bytes => .{ .name = msm.METRIC_RESPONSE_ERASED_BYTES, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative },
        .sensitive_response_write_failures => cum(msm.METRIC_RESPONSE_WRITE_FAILURES, 1),
        .runner_failures => streamedMeta(cum(mr.FAILURES_NAME, 1), mr.MAX_SLOTS * mr.REASON_LABELS.len),
        .runner_failures_overflow => streamedMeta(cum(mr.FAILURES_OVERFLOW_NAME, 1), 1),
        .runner_executions => streamedMeta(cum(mr.EXECUTIONS_NAME, 1), mr.MAX_SLOTS * mr.OUTCOME_LABELS.len),
        .runner_last_seen_seconds => streamedMeta(gauge(mr.LAST_SEEN_NAME, semconv.UNIT_SECONDS, 1), mr.MAX_SLOTS),
        .runner_active_leases => streamedMeta(gauge(mr.ACTIVE_LEASES_NAME, semconv.UNIT_LEASES, 1), mr.MAX_SLOTS),
    };
}

pub const METRIC_ID_COUNT = @typeInfo(MetricId).@"enum".fields.len;

const METAS = blk: {
    var metas: [METRIC_ID_COUNT]MetricMeta = undefined;
    for (0..METRIC_ID_COUNT) |i| metas[i] = buildMeta(@enumFromInt(i));
    break :blk metas;
};

pub fn metaFor(id: MetricId) MetricMeta {
    return METAS[@intFromEnum(id)];
}

// ---------------------------------------------------------------------------
// Series-ceiling arithmetic — derived, never chosen (M-free by construction:
// every term below re-derives from the declarations above).
// ---------------------------------------------------------------------------

/// The evented cost families' series sub-budget. This is the pre-existing
/// aggregator ceiling, kept verbatim so the model-attribution cap derived
/// from it is unchanged by runtime-family growth. Changing this number is an
/// attribution decision, never a side effect of adding a family.
pub const COST_SERIES_BUDGET: usize = 256;

/// Comptime sum of every fixed-label runtime family's worst case.
pub const RUNTIME_FIXED_SERIES: usize = blk: {
    var total: usize = 0;
    for (METAS) |meta| {
        if (!meta.cost and !meta.streamed) total += meta.max_series;
    }
    break :blk total;
};

/// The aggregator's derived distinct-series ceiling: the cost sub-budget plus
/// exactly what the declared runtime families can occupy. A new family grows
/// this ceiling instead of silently evicting cost attribution.
pub const MAX_SERIES: usize = COST_SERIES_BUDGET + RUNTIME_FIXED_SERIES;

/// Upper bound on the aggregator's static accumulator array — a memory bound
/// on a small machine, not a tuning knob. A declaration set whose worst case
/// exceeds it fails the build here rather than shedding series at runtime.
pub const AGGREGATOR_HARD_CAP: usize = 1024;

/// Streamed (per-runner) worst case: bounded by the slot table's own
/// capacity, reused rather than re-declared.
pub const STREAMED_SERIES_WORST_CASE: usize = blk: {
    var total: usize = 0;
    for (METAS) |meta| {
        if (meta.streamed) total += meta.max_series;
    }
    break :blk total;
};

comptime {
    std.debug.assert(MAX_SERIES <= AGGREGATOR_HARD_CAP);
    // A zero-width runtime set would mean the registry lost its declarations.
    std.debug.assert(RUNTIME_FIXED_SERIES > 0);
    std.debug.assert(STREAMED_SERIES_WORST_CASE > 0);
    // The streamed appender serializes from a Sample, which carries no bucket
    // state — a streamed histogram would slice empty bucket_counts out of
    // bounds at flush time, so the registry refuses the combination here.
    for (METAS) |meta| std.debug.assert(!(meta.streamed and meta.kind == .histogram));
}
