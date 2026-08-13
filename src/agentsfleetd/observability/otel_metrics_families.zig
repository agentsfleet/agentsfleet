//! The closed metric-family registry for the OTLP exporter.
//!
//! Every family the daemon exports — evented cost families and flush-time
//! runtime families alike — is declared here, once: its wire identity
//! (`MetricMeta`) and its label dimensions (`dimsFor`). The wire serializer
//! (otel_metrics_payload.zig), the aggregator's series ceiling
//! (otel_metrics_aggregate.zig), the model-attribution budget
//! (otel_metrics_cardinality.zig), the generated instrument layer
//! (otel_instruments.zig — storage cells, typed writer, collect loop), and the
//! census/namespace guard tests all read this table, so a family cannot exist
//! on the wire without being declared — and neither the ceiling arithmetic nor
//! the storage layout can drift from what is actually exported.
//!
//! Collection shapes: `evented` — samples through the lock-free ring, with
//! `cost` families separately budgeted by `COST_SERIES_BUDGET` so runtime
//! growth can never shrink model attribution; `streamed` — pre-aggregated
//! per-runner families serialized straight from the slot table, never entering
//! the Aggregator; `live_read` —
//! read at flush time by an explicit collect hook (pool stats, resident-set
//! probe, flush-thread liveness), no generated cell so absence semantics stay
//! with the source; none of the above — fixed-label families stored in the
//! generated cell table, `max_series` derived from the declared dimensions.

const std = @import("std");
const semconv = @import("semconv.zig");
const meta_mod = @import("otel_metric_meta.zig");
const dims = @import("otel_metrics_dims.zig");
const mot = @import("metrics_otel.zig");
const mc = @import("metrics_counters.zig");
const mr = @import("metrics_runner.zig");
const mm = @import("metrics_memory.zig");
const msm = @import("metrics_sensitive_memory.zig");
const mt = @import("metrics_trace.zig");
const mrv = @import("metrics_repair_verification.zig");
const ls = @import("library_stages.zig");

pub const MetricKind = meta_mod.MetricKind;
pub const Temporality = meta_mod.Temporality;
pub const Scale = meta_mod.Scale;
pub const MetricMeta = meta_mod.MetricMeta;

pub const MetricId = enum {
    // Evented cost families (delta, through the ring).
    invoke_agent_duration,
    token_usage,
    cache_read_token_usage,
    credit_consumed,
    samples_dropped,
    // Evented repair-verification latency families.
    repair_production_to_queue,
    repair_queue_to_completion,
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
    // Production-repair verification.
    repair_provider_results,
    repair_correlations,
    repair_verification_intents_created,
    repair_dispatch_retried,
    repair_synthetic_events,
    repair_verifier_runs,
    repair_dispatch_due_batch,
    repair_dispatch_oldest_age,
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

const cum = meta_mod.cumulative;
const gauge = meta_mod.gauge;
const cumBytes = meta_mod.cumulativeBytes;
const liveRead = meta_mod.liveRead;
const streamedMeta = meta_mod.streamed;
const costMeta = meta_mod.cost;
const eventedMeta = meta_mod.evented;

// Operator help prose (the retired Prometheus HELP knowledge) rides the
// family rows below, beside the wire identity it describes.

fn buildMeta(id: MetricId) MetricMeta {
    return switch (id) {
        .invoke_agent_duration => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_DURATION, .unit = semconv.UNIT_SECONDS, .kind = .histogram, .bounds = &semconv.DURATION_BUCKET_BOUNDS_MS, .scale = .millis_to_seconds }),
        .token_usage => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_TOKEN_USAGE, .unit = semconv.UNIT_TOKENS, .kind = .histogram, .bounds = &semconv.TOKEN_BUCKET_BOUNDS }),
        .cache_read_token_usage => costMeta(.{ .name = semconv.METRIC_INVOKE_AGENT_CACHE_READ, .unit = semconv.UNIT_TOKENS, .kind = .histogram, .bounds = &semconv.TOKEN_BUCKET_BOUNDS }),
        .credit_consumed => costMeta(.{ .name = semconv.METRIC_BILLING_CREDIT_CONSUMED, .unit = semconv.UNIT_NANOCREDITS, .kind = .sum, .monotonic = true }),
        .samples_dropped => costMeta(.{ .name = semconv.METRIC_SAMPLES_DROPPED, .unit = semconv.UNIT_COUNT, .kind = .sum, .monotonic = true }),
        .repair_production_to_queue => eventedMeta(.{ .name = mrv.PRODUCTION_TO_QUEUE_NAME, .unit = semconv.UNIT_SECONDS, .kind = .histogram, .bounds = &mrv.HISTOGRAM_BOUNDS_MS, .scale = .millis_to_seconds }),
        .repair_queue_to_completion => eventedMeta(.{ .name = mrv.QUEUE_TO_COMPLETE_NAME, .unit = semconv.UNIT_SECONDS, .kind = .histogram, .bounds = &mrv.HISTOGRAM_BOUNDS_MS, .scale = .millis_to_seconds }),
        .api_backpressure_rejections => cum(semconv.METRIC_API_BACKPRESSURE_REJECTIONS),
        .api_in_flight_requests => gauge(semconv.METRIC_API_IN_FLIGHT_REQUESTS, semconv.UNIT_REQUESTS),
        .sse_backpressure_rejections => cum(semconv.METRIC_SSE_BACKPRESSURE_REJECTIONS),
        .sse_in_flight_streams => gauge(semconv.METRIC_SSE_IN_FLIGHT_STREAMS, semconv.UNIT_STREAMS),
        .sse_dropped_frames => cum(semconv.METRIC_SSE_DROPPED_FRAMES),
        .sse_hub_reconnects => cum(semconv.METRIC_SSE_HUB_RECONNECTS),
        // Liveness: read by hook as constant 1 — the flush thread only runs inside a live daemon.
        .worker_running => liveRead(gauge(semconv.METRIC_WORKER_RUNNING, semconv.UNIT_WORKERS)),
        .fleet_triggered => cum(semconv.METRIC_FLEET_TRIGGERED),
        // HTTP request spans suppressed by the bounded trace admission policy.
        .http_trace_suppressed => cum(mt.SUPPRESSED_NAME),
        // Current entries buffered for OTLP export, per signal.
        .otlp_queue_depth => gauge(mot.QUEUE_DEPTH_NAME, semconv.UNIT_ENTRIES),
        // Entries discarded locally or reported rejected by the OTLP backend.
        .otlp_entries_discarded => cum(mot.DISCARDED_NAME),
        // Attributes omitted to keep series bounded and standard; the measurement itself still exported.
        .otel_attribute_omitted => cum(mot.ATTRIBUTE_OMITTED_NAME),
        .signup_bootstrapped => cum(semconv.METRIC_SIGNUP_BOOTSTRAPPED),
        .signup_replayed => cum(semconv.METRIC_SIGNUP_REPLAYED),
        .signup_failed => cum(semconv.METRIC_SIGNUP_FAILED),
        // Lease polls served, the denominator for the per-poll cost families.
        .lease_polls => cum(mc.LEASE_POLLS_NAME),
        // Fleets examined across all lease polls; divide by lease polls for mean fan-out.
        .lease_poll_candidates_scanned => cum(mc.CANDIDATES_SCANNED_NAME),
        // Postgres round-trips issued on the lease path; an idle poll must contribute zero.
        .lease_poll_db_roundtrips => cum(mc.DB_ROUNDTRIPS_NAME),
        // Fleets in the shared readiness index; NOT summable across replicas (all sample the same index).
        .fleet_ready_depth => gauge(mc.READY_DEPTH_NAME, semconv.UNIT_FLEETS),
        // Readiness index writes that failed against Redis; the log line carries which of mark/clear.
        .fleet_ready_write_failures => cum(mc.READY_WRITE_FAILURES_NAME),
        // Rows deleted by the retention sweep; a flat line on a busy plane means it stopped or always fails.
        .runner_retention_swept => cum(mc.RETENTION_SWEPT_NAME),
        // Sweep cycles that ended in error — rising means history is no longer pruned.
        .runner_retention_sweep_failures => cum(mc.RETENTION_SWEEP_FAILURES_NAME),
        // Unregister calls that failed during an account purge — an erased tenant may still have a firing timer.
        .account_teardown_unregister_failures => cum(mc.TEARDOWN_UNREGISTER_FAILURES_NAME),
        .repair_provider_results => cum(mrv.PROVIDER_NAME),
        .repair_correlations => cum(mrv.CORRELATION_NAME),
        .repair_verification_intents_created => cum(mrv.INTENTS_CREATED_NAME),
        .repair_dispatch_retried => cum(mrv.DISPATCH_RETRIED_NAME),
        .repair_synthetic_events => cum(mrv.EVENT_NAME),
        .repair_verifier_runs => cum(mrv.VERIFIER_NAME),
        .repair_dispatch_due_batch => gauge(mrv.DUE_BATCH_NAME, semconv.UNIT_ENTRIES),
        .repair_dispatch_oldest_age => gauge(mrv.BACKLOG_AGE_NAME, semconv.UNIT_SECONDS),
        // Seconds in one library read stage; divide by the observations counter for mean cost.
        .library_stage_duration => .{ .name = ls.STAGE_DURATION_NAME, .unit = semconv.UNIT_SECONDS, .kind = .sum, .monotonic = true, .temporality = .cumulative, .scale = .nanos_to_seconds },
        // Completed library read stages — the denominator for the duration sum.
        .library_stage_observations => cum(ls.STAGE_OBSERVATIONS_NAME),
        // Library reads by surface and terminal outcome, exactly once per request.
        .library_read_outcome => cum(ls.READ_OUTCOME_NAME),
        // Pool acquisitions by result; unlabelled — a starving pool is process-wide.
        .library_pool_result => cum(ls.POOL_RESULT_NAME),
        // Cache dispositions; global cache only, no tenant or request identity.
        .library_cache_outcome => cum(ls.CACHE_OUTCOME_NAME),
        // Encoded response bytes produced by library reads, by surface.
        .library_payload_bytes => .{ .name = ls.PAYLOAD_BYTES_NAME, .unit = semconv.UNIT_BYTES, .kind = .sum, .monotonic = true, .temporality = .cumulative },
        // Rows materialised into library read projections, by surface.
        .library_results => cum(ls.RESULTS_NAME),
        .redis_pool_active => liveRead(gauge(semconv.METRIC_REDIS_POOL_ACTIVE, semconv.UNIT_CONNECTIONS)),
        .redis_pool_idle => liveRead(gauge(semconv.METRIC_REDIS_POOL_IDLE, semconv.UNIT_CONNECTIONS)),
        .redis_pool_dials => liveRead(cum(semconv.METRIC_REDIS_POOL_DIALS)),
        .redis_pool_overflow_dials => liveRead(cum(semconv.METRIC_REDIS_POOL_OVERFLOW_DIALS)),
        .redis_pool_poisoned_connections => liveRead(cum(semconv.METRIC_REDIS_POOL_POISONED)),
        .redis_pool_reconnects => liveRead(cum(semconv.METRIC_REDIS_POOL_RECONNECTS)),
        .redis_pool_forced_closes => liveRead(cum(semconv.METRIC_REDIS_POOL_FORCED_CLOSES)),
        .redis_pool_acquire_timeouts => liveRead(cum(semconv.METRIC_REDIS_POOL_ACQUIRE_TIMEOUTS)),
        .memory_entries_captured => cum(mm.MEM_CAPTURED_NAME),
        .memory_push_failures => cum(mm.MEM_PUSH_FAIL_NAME),
        .memory_hydration_window_entries => gauge(mm.MEM_HYDRATION_NAME, semconv.UNIT_ENTRIES),
        .memory_hydration_dropped_entries => cum(mm.HYDRATION_DROPPED_ENTRIES_NAME),
        .memory_hydration_dropped_bytes => cumBytes(mm.HYDRATION_DROPPED_BYTES_NAME),
        .memory_cap_evictions => cum(mm.CAP_EVICTIONS_NAME),
        .memory_capture_truncated => cum(mm.CAPTURE_TRUNCATED_NAME),
        .memory_capture_skipped => cum(mm.CAPTURE_SKIPPED_NAME),
        .memory_search_zero_hits => cum(mm.SEARCH_ZERO_HITS_NAME),
        .process_resident_memory_bytes => liveRead(gauge(msm.METRIC_PROCESS_RESIDENT_MEMORY, semconv.UNIT_BYTES)),
        .sensitive_request_erased_bytes => cumBytes(msm.METRIC_REQUEST_ERASED_BYTES),
        .sensitive_response_erased_bytes => cumBytes(msm.METRIC_RESPONSE_ERASED_BYTES),
        .sensitive_response_write_failures => cum(msm.METRIC_RESPONSE_WRITE_FAILURES),
        .runner_failures => streamedMeta(cum(mr.FAILURES_NAME), mr.MAX_SLOTS * mr.REASON_LABELS.len),
        .runner_failures_overflow => streamedMeta(cum(mr.FAILURES_OVERFLOW_NAME), 1),
        .runner_executions => streamedMeta(cum(mr.EXECUTIONS_NAME), mr.MAX_SLOTS * mr.OUTCOME_LABELS.len),
        .runner_last_seen_seconds => streamedMeta(gauge(mr.LAST_SEEN_NAME, semconv.UNIT_SECONDS), mr.MAX_SLOTS),
        .runner_active_leases => streamedMeta(gauge(mr.ACTIVE_LEASES_NAME, semconv.UNIT_LEASES), mr.MAX_SLOTS),
    };
}

pub const METRIC_ID_COUNT = @typeInfo(MetricId).@"enum".fields.len;

const METAS = blk: {
    var metas: [METRIC_ID_COUNT]MetricMeta = undefined;
    for (0..METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        var meta = buildMeta(id);
        // Fixed-label (and live-read) runtime families derive their worst case
        // from the declared dimensions; cost and streamed budgets stay theirs.
        if (!meta.cost and !meta.streamed) meta.max_series = dims.fixedDimProduct(dims.dimsFor(id));
        metas[i] = meta;
    }
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
        if (!meta.evented and !meta.streamed) total += meta.max_series;
    }
    break :blk total;
};

/// Comptime sum of non-cost evented runtime families' worst case.
pub const RUNTIME_EVENTED_SERIES: usize = blk: {
    var total: usize = 0;
    for (METAS) |meta| {
        if (meta.evented and !meta.cost) total += meta.max_series;
    }
    break :blk total;
};

/// The aggregator's derived distinct-series ceiling: the cost sub-budget plus
/// exactly what the declared runtime families can occupy. A new family grows
/// this ceiling instead of silently evicting cost attribution.
pub const MAX_SERIES: usize = COST_SERIES_BUDGET + RUNTIME_EVENTED_SERIES + RUNTIME_FIXED_SERIES;

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
    for (0..METRIC_ID_COUNT) |i| {
        const id: MetricId = @enumFromInt(i);
        const meta = METAS[i];
        // The streamed appender serializes from a Sample, which carries no
        // bucket state — a streamed histogram would slice empty bucket_counts
        // out of bounds at flush time, so the registry refuses the combination.
        std.debug.assert(!(meta.streamed and meta.kind == .histogram));
        // One inline dynamic value per sample — see dims.MAX_DYNAMIC_DIMS.
        std.debug.assert(dims.validDims(dims.dimsFor(id)));
        for (dims.dimsFor(id)) |dim| {
            // Storage cells exist only for closed dimensions; a dynamic
            // dimension on a cell-stored family would have unbounded cells.
            if (!meta.evented and !meta.streamed) std.debug.assert(dim == .fixed);
            // Hooked families read live state; declared dimensions would
            // promise cells the hook never fills.
            if (meta.live_read) std.debug.assert(false);
        }
    }
}
