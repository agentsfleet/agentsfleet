//! Flush-time collector for the runtime metric families.
//!
//! Two jobs, both invoked from otel_metrics.zig's flush path:
//!   - `collect` snapshots every fixed-label runtime source (guardrail
//!     counters, trace admission, exporter self-health, memory planes, Redis
//!     pool, library read families) into the flush window's Aggregator — one
//!     Sample per labelset, zero values included, so a dashboard series stays
//!     live between increments.
//!   - `appendStreamedRunnerFamilies` streams the per-runner families straight
//!     from metrics_runner.zig's slot table into the OTLP envelope, bypassing
//!     the Aggregator (a 4096-slot table would swamp its series ceiling).
//!     Zero-count cells are skipped to bound the payload — the retired
//!     Prometheus renderer likewise exposed only cells that had moved.
//!
//! Which family takes which shape is declared in otel_metrics_families.zig
//! (`streamed` flag); a family collected here must stay declared there or the
//! census tests fail the build.

const std = @import("std");
const common = @import("common");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const mc = @import("metrics_counters.zig");
const mt = @import("metrics_trace.zig");
const mot = @import("metrics_otel.zig");
const mm = @import("metrics_memory.zig");
const msm = @import("metrics_sensitive_memory.zig");
const mrp = @import("metrics_redis_pool.zig");
const mr = @import("metrics_runner.zig");
const ls = @import("library_stages.zig");

// Label keys for the dimensions this collector emits. Library families reuse
// ls.LABEL_* so each wire string keeps one home.
const LABEL_RUNNER = "runner_id";
const LABEL_REASON = "reason";
const LABEL_OUTCOME = "outcome";
const LABEL_SIGNAL = "signal";
const LABEL_ATTRIBUTE = "attribute";

/// The flush thread only runs inside a live daemon, so the liveness gauge is
/// the constant the retired pull handler hardcoded as `true`.
const WORKER_RUNNING_VALUE: i64 = 1;

/// Snapshot counters are u64; Sample.value is i64. Saturate rather than trap:
/// telemetry, not money.
fn satCast(value: u64) i64 {
    return @intCast(@min(value, std.math.maxInt(i64)));
}

fn push(agg: *aggregate.Aggregator, id: payload.MetricId, value: u64) void {
    agg.add(payload.newSample(id, satCast(value)));
}

fn push1(agg: *aggregate.Aggregator, id: payload.MetricId, value: u64, key: []const u8, val: []const u8) void {
    var s = payload.newSample(id, satCast(value));
    _ = payload.addLabel(&s, key, val);
    agg.add(s);
}

fn push2(agg: *aggregate.Aggregator, id: payload.MetricId, value: u64, k1: []const u8, v1: []const u8, k2: []const u8, v2: []const u8) void {
    var s = payload.newSample(id, satCast(value));
    _ = payload.addLabel(&s, k1, v1);
    _ = payload.addLabel(&s, k2, v2);
    agg.add(s);
}

/// Push one Sample per fixed-label runtime family labelset into `agg`.
/// Called once per flush window, on the flush thread, after the evented ring
/// is drained and before the Aggregator is serialized.
pub fn collect(agg: *aggregate.Aggregator) void {
    const counters = mc.snapshot();
    collectCounters(agg, counters);
    collectSignup(agg, counters);
    collectTrace(agg);
    collectExporterHealth(agg);
    collectMemory(agg);
    collectSensitive(agg);
    collectRedisPool(agg);
    collectLibrary(agg);
}

fn collectCounters(agg: *aggregate.Aggregator, s: mc.Snapshot) void {
    push(agg, .api_backpressure_rejections, s.api_backpressure_rejections_total);
    push(agg, .api_in_flight_requests, s.api_in_flight_requests);
    push(agg, .sse_backpressure_rejections, s.sse_backpressure_rejections_total);
    push(agg, .sse_in_flight_streams, s.sse_in_flight_streams);
    push(agg, .sse_dropped_frames, s.sse_dropped_frames_total);
    push(agg, .sse_hub_reconnects, s.sse_hub_reconnects_total);
    push(agg, .fleet_triggered, s.fleet_triggered_total);
    agg.add(payload.newSample(.worker_running, WORKER_RUNNING_VALUE));
    push(agg, .lease_polls, s.lease_polls_total);
    push(agg, .lease_poll_candidates_scanned, s.lease_poll_candidates_scanned_total);
    push(agg, .lease_poll_db_roundtrips, s.lease_poll_db_roundtrips_total);
    push(agg, .fleet_ready_depth, s.fleet_ready_depth);
    push(agg, .fleet_ready_write_failures, s.fleet_ready_write_failures_total);
    push(agg, .runner_retention_swept, s.runner_retention_swept_total);
    push(agg, .runner_retention_sweep_failures, s.runner_retention_sweep_failures_total);
    push(agg, .account_teardown_unregister_failures, s.account_teardown_unregister_failures_total);
}

fn collectSignup(agg: *aggregate.Aggregator, s: mc.Snapshot) void {
    push(agg, .signup_bootstrapped, s.signup_bootstrapped_total);
    push(agg, .signup_replayed, s.signup_replayed_total);
    // Field order below must match SIGNUP_FAIL_REASON_LABELS (= SignupFailReason
    // declaration order) — the pairwise loop is what ties value to reason.
    const failed = [mc.SIGNUP_FAIL_REASON_LABELS.len]u64{
        s.signup_failed_bad_sig_total,
        s.signup_failed_stale_ts_total,
        s.signup_failed_missing_email_total,
        s.signup_failed_db_error_total,
        s.signup_failed_pool_unavailable_total,
        s.signup_failed_metadata_writeback_total,
    };
    for (mc.SIGNUP_FAIL_REASON_LABELS, failed) |reason, value| {
        push1(agg, .signup_failed, value, LABEL_REASON, reason);
    }
}

fn collectTrace(agg: *aggregate.Aggregator) void {
    const s = mt.snapshot();
    // Same order-pairing contract as collectSignup: Snapshot field order is
    // SUPPRESSION_REASON_LABELS order.
    const values = [mt.SUPPRESSION_REASON_LABELS.len]u64{
        s.noisy_route_total,
        s.runner_rejection_budget_total,
        s.server_error_budget_total,
        s.sampled_success_budget_total,
        s.sample_miss_total,
    };
    for (mt.SUPPRESSION_REASON_LABELS, values) |reason, value| {
        push1(agg, .http_trace_suppressed, value, LABEL_REASON, reason);
    }
}

fn collectExporterHealth(agg: *aggregate.Aggregator) void {
    const s = mot.snapshot();
    for (mot.SIGNALS, 0..) |signal, si| {
        push1(agg, .otlp_queue_depth, s.queue_depth[si], LABEL_SIGNAL, @tagName(signal));
        for (mot.DISCARD_REASONS, 0..) |reason, ri| {
            push2(agg, .otlp_entries_discarded, s.discarded[si][ri], LABEL_SIGNAL, @tagName(signal), LABEL_REASON, @tagName(reason));
        }
    }
    for (mot.OMITTED_ATTRIBUTES, 0..) |attribute, ai| {
        for (mot.OMISSION_REASONS, 0..) |reason, ri| {
            push2(agg, .otel_attribute_omitted, s.attribute_omitted[ai][ri], LABEL_ATTRIBUTE, attribute.label(), LABEL_REASON, reason.label());
        }
    }
}

fn collectMemory(agg: *aggregate.Aggregator) void {
    const s = mm.snapshot();
    push(agg, .memory_entries_captured, s.captured_total);
    push(agg, .memory_push_failures, s.push_failures_total);
    // Gauge; clamp the transient <0 the same way the retired renderer did.
    agg.add(payload.newSample(.memory_hydration_window_entries, @max(0, s.hydration_entries)));
    push(agg, .memory_hydration_dropped_entries, s.hydration_dropped_entries_total);
    push(agg, .memory_hydration_dropped_bytes, s.hydration_dropped_bytes_total);
    push(agg, .memory_cap_evictions, s.cap_evictions_total);
    push(agg, .memory_capture_truncated, s.capture_truncated_total);
    push(agg, .memory_capture_skipped, s.capture_skipped_total);
    push(agg, .memory_search_zero_hits, s.search_zero_hits_total);
}

fn collectSensitive(agg: *aggregate.Aggregator) void {
    // Resident set size is platform-dependent; when the platform can't report
    // it the family is absent rather than a fake zero.
    if (common.rss.currentBytes()) |resident_bytes| {
        push(agg, .process_resident_memory_bytes, resident_bytes);
    }
    const s = msm.snapshot();
    push(agg, .sensitive_request_erased_bytes, s.request_erased_bytes_total);
    push(agg, .sensitive_response_erased_bytes, s.response_erased_bytes_total);
    push(agg, .sensitive_response_write_failures, s.response_write_failures_total);
}

fn collectRedisPool(agg: *aggregate.Aggregator) void {
    // No registered Pool (early-boot flush, or post-teardown) → no redis_pool_*
    // families this window, matching metrics_redis_pool.zig's null story.
    const stats = mrp.snapshot() orelse return;
    push(agg, .redis_pool_active, stats.active);
    push(agg, .redis_pool_idle, stats.idle);
    push(agg, .redis_pool_dials, stats.dials_total);
    push(agg, .redis_pool_overflow_dials, stats.overflow_dials_total);
    push(agg, .redis_pool_poisoned_connections, stats.poisoned_connections_total);
    push(agg, .redis_pool_reconnects, stats.reconnects_total);
    push(agg, .redis_pool_forced_closes, stats.forced_closes_total);
    push(agg, .redis_pool_acquire_timeouts, stats.acquire_timeouts_total);
}

fn collectLibrary(agg: *aggregate.Aggregator) void {
    const s = ls.snapshot();
    for (ls.SURFACE_LABELS, 0..) |surface, si| {
        for (ls.STAGE_LABELS, 0..) |stage, sti| {
            // duration_ns stays integer; the family's declared scale converts
            // to seconds at serialization.
            push2(agg, .library_stage_duration, s.stages[si][sti].duration_ns, ls.LABEL_SURFACE, surface, ls.LABEL_STAGE, stage);
            push2(agg, .library_stage_observations, s.stages[si][sti].count, ls.LABEL_SURFACE, surface, ls.LABEL_STAGE, stage);
        }
        for (ls.OUTCOME_LABELS, 0..) |outcome, oi| {
            push2(agg, .library_read_outcome, s.read_outcomes[si][oi], ls.LABEL_SURFACE, surface, ls.LABEL_OUTCOME, outcome);
        }
        push1(agg, .library_payload_bytes, s.payload_bytes[si], ls.LABEL_SURFACE, surface);
        push1(agg, .library_results, s.results[si], ls.LABEL_SURFACE, surface);
    }
    for (ls.POOL_RESULT_LABELS, 0..) |label, i| {
        push1(agg, .library_pool_result, s.pool_results[i], ls.LABEL_POOL_RESULT, label);
    }
    for (ls.CACHE_LABELS, 0..) |label, i| {
        push1(agg, .library_cache_outcome, s.cache_outcomes[i], ls.LABEL_CACHE, label);
    }
}

// ── Streamed per-runner families ────────────────────────────────────────────

/// Comma discipline + budget tracking for metric objects appended after the
/// aggregated series inside one OTLP envelope. The payload lives in a fixed
/// arena: when another series no longer fits, streaming stops and the rest of
/// the runner set is counted as shed — a partially streamed window must never
/// become a failed serialization that also discards the drained evented
/// samples, and must never leave the batch as broken JSON.
const StreamState = struct {
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    times: payload.WireTimes,
    need_comma: bool,
    result: payload.ExtraAppendResult,
    exhausted: bool,

    fn emit(self: *StreamState, sample: payload.Sample) void {
        if (self.exhausted) {
            self.result.shed += 1;
            return;
        }
        // The per-runner families are sums and gauges only, so a Sample carries
        // everything a Series needs; histogram fields stay zeroed/empty.
        const series = payload.Series{
            .id = sample.id,
            .labels = sample.labels[0..sample.label_count],
            .sum_value = sample.value,
            .hist_count = 0,
            .hist_sum = 0,
            .bucket_counts = &.{},
        };
        const rollback_len = self.list.items.len;
        self.append(series) catch {
            // Roll the partial object back so the envelope stays valid JSON,
            // then stop streaming for this window; the shed count surfaces
            // through the discard health family.
            self.list.shrinkRetainingCapacity(rollback_len);
            self.exhausted = true;
            self.result.shed += 1;
        };
    }

    fn append(self: *StreamState, series: payload.Series) !void {
        if (self.need_comma) try self.list.appendSlice(self.alloc, ",");
        try payload.appendSeriesMetric(self.list, self.alloc, series, self.times);
        self.need_comma = true;
        self.result.appended += 1;
    }
};

/// payload.ExtraAppendFn: append the per-runner families for every live slot,
/// plus one overflow series when any failure increment ever missed the table.
/// Returns how many objects were appended and how many were shed at the
/// payload budget.
pub fn appendStreamedRunnerFamilies(
    list: *std.ArrayList(u8),
    alloc: std.mem.Allocator,
    times: payload.WireTimes,
    wrote_any: bool,
) anyerror!payload.ExtraAppendResult {
    var st = StreamState{
        .list = list,
        .alloc = alloc,
        .times = times,
        .need_comma = wrote_any,
        .result = .{},
        .exhausted = false,
    };
    var it = mr.liveSlots();
    while (it.next()) |slot| appendSlotFamilies(&st, slot);
    // Runners routed past the slot table keep their reason/outcome detail
    // under the shared identity, so fleet-wide sums stay complete.
    const spill = mr.overflowCounts();
    appendCounterCells(&st, mr.ID_OTHER, spill.failures, spill.executions);
    const overflow = mr.overflowTotal();
    if (overflow > 0) st.emit(payload.newSample(.runner_failures_overflow, satCast(overflow)));
    return st.result;
}

fn appendCounterCells(
    st: *StreamState,
    runner_id: []const u8,
    failures: [mr.REASON_LABELS.len]u64,
    executions: [mr.OUTCOME_LABELS.len]u64,
) void {
    for (mr.REASON_LABELS, failures) |reason, count| {
        if (count == 0) continue;
        var s = payload.newSample(.runner_failures, satCast(count));
        _ = payload.addLabel(&s, LABEL_RUNNER, runner_id);
        _ = payload.addLabel(&s, LABEL_REASON, reason);
        st.emit(s);
    }
    for (mr.OUTCOME_LABELS, executions) |outcome, count| {
        if (count == 0) continue;
        var s = payload.newSample(.runner_executions, satCast(count));
        _ = payload.addLabel(&s, LABEL_RUNNER, runner_id);
        _ = payload.addLabel(&s, LABEL_OUTCOME, outcome);
        st.emit(s);
    }
}

fn appendSlotFamilies(st: *StreamState, slot: mr.SlotView) void {
    appendCounterCells(st, slot.runner_id, slot.failures, slot.executions);
    // null = never seen; the retired renderer skipped the gauge for such slots.
    if (slot.last_seen_seconds) |age_s| {
        var s = payload.newSample(.runner_last_seen_seconds, age_s);
        _ = payload.addLabel(&s, LABEL_RUNNER, slot.runner_id);
        st.emit(s);
    }
    // A live runner's lease level is a fact even at zero — absence of this
    // gauge means the runner (or the exporter) is gone, never "no leases".
    // Transient sub-zero reads from the best-effort decrement clamp to zero.
    var leases = payload.newSample(.runner_active_leases, @max(slot.active_leases, 0));
    _ = payload.addLabel(&leases, LABEL_RUNNER, slot.runner_id);
    st.emit(leases);
}
