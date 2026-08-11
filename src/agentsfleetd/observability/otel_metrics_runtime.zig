//! Flush-time collector for the runtime metric families.
//!
//! Two jobs, both invoked from otel_metrics.zig's flush path:
//!   - `collect` delegates to the generated instrument layer
//!     (otel_instruments.zig), which emits one Sample per registry-declared
//!     cell — zero values included, so a dashboard series stays live between
//!     increments — then runs this module's live-read hooks: sources that
//!     cannot be module atomics (the Redis pool snapshot, absent until a pool
//!     registers; the resident-set probe, absent when the platform cannot
//!     report; the flush-thread liveness constant). A hook's absence keeps its
//!     family out of the window — never a fake zero.
//!   - `appendStreamedRunnerFamilies` streams the per-runner families straight
//!     from metrics_runner.zig's slot table into the OTLP envelope, bypassing
//!     the Aggregator (a 4096-slot table would swamp its series ceiling).
//!     Zero-count cells are skipped to bound the payload — the retired
//!     Prometheus renderer likewise exposed only cells that had moved.
//!
//! Which family takes which shape is declared in otel_metrics_families.zig
//! (`streamed` / `live_read` flags); a family collected here must stay
//! declared there or the census tests fail the build.

const std = @import("std");
const common = @import("common");
const payload = @import("otel_metrics_payload.zig");
const aggregate = @import("otel_metrics_aggregate.zig");
const dims = @import("otel_metrics_dims.zig");
const instruments = @import("otel_instruments.zig");
const mrp = @import("metrics_redis_pool.zig");
const mr = @import("metrics_runner.zig");

/// The flush thread only runs inside a live daemon, so the liveness gauge is
/// the constant the retired pull handler hardcoded as `true`.
const WORKER_RUNNING_VALUE: i64 = 1;

comptime {
    // A runner id longer than the sample's inline buffer would make every
    // setDynamicLabel below return false — all runners would merge into one
    // anonymous series, uncounted. Refuse the combination at build time.
    std.debug.assert(mr.ID_LEN <= payload.MAX_LABEL_VAL);
}

/// Live-read sources, run by the instrument layer after the generated cells
/// so their families join the same flush window. Order matches the registry's
/// family declaration order for a stable envelope layout.
const HOOKS = [_]instruments.CollectHook{
    hookWorkerRunning,
    hookRedisPool,
    hookResidentMemory,
};

/// Push one Sample per fixed-label runtime family labelset into `agg`.
/// Called once per flush window, on the flush thread, after the evented ring
/// is drained and before the Aggregator is serialized.
pub fn collect(agg: *aggregate.Aggregator) void {
    instruments.collect(agg, &HOOKS);
}

fn emit(agg: *aggregate.Aggregator, id: payload.MetricId, value: u64) void {
    agg.add(payload.newSample(id, payload.satCast(value)));
}

fn hookWorkerRunning(agg: *aggregate.Aggregator) void {
    agg.add(payload.newSample(.worker_running, WORKER_RUNNING_VALUE));
}

fn hookRedisPool(agg: *aggregate.Aggregator) void {
    // No registered Pool (early-boot flush, or post-teardown) → no redis_pool_*
    // families this window, matching metrics_redis_pool.zig's null story.
    const stats = mrp.snapshot() orelse return;
    emit(agg, .redis_pool_active, stats.active);
    emit(agg, .redis_pool_idle, stats.idle);
    emit(agg, .redis_pool_dials, stats.dials_total);
    emit(agg, .redis_pool_overflow_dials, stats.overflow_dials_total);
    emit(agg, .redis_pool_poisoned_connections, stats.poisoned_connections_total);
    emit(agg, .redis_pool_reconnects, stats.reconnects_total);
    emit(agg, .redis_pool_forced_closes, stats.forced_closes_total);
    emit(agg, .redis_pool_acquire_timeouts, stats.acquire_timeouts_total);
}

fn hookResidentMemory(agg: *aggregate.Aggregator) void {
    // Resident set size is platform-dependent; when the platform can't report
    // it the family is absent rather than a fake zero.
    if (common.rss.currentBytes()) |resident_bytes| {
        emit(agg, .process_resident_memory_bytes, resident_bytes);
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
            .dynamic = sample.dynamic[0..sample.dynamic_len],
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
    if (overflow > 0) st.emit(payload.newSample(.runner_failures_overflow, payload.satCast(overflow)));
    return st.result;
}

fn appendCounterCells(
    st: *StreamState,
    runner_id: []const u8,
    failures: [mr.REASON_LABELS.len]u64,
    executions: [mr.OUTCOME_LABELS.len]u64,
) void {
    // inline so the reason/outcome labels resolve to comptime interned
    // indices — the label tables are the closed sets these arrays came from.
    inline for (mr.REASON_LABELS, 0..) |reason, i| {
        if (failures[i] != 0) {
            var s = payload.newSample(.runner_failures, payload.satCast(failures[i]));
            _ = payload.setDynamicLabel(&s, dims.LABEL_RUNNER, runner_id);
            _ = payload.addInternedLabel(&s, dims.LABEL_REASON, reason);
            st.emit(s);
        }
    }
    inline for (mr.OUTCOME_LABELS, 0..) |outcome, i| {
        if (executions[i] != 0) {
            var s = payload.newSample(.runner_executions, payload.satCast(executions[i]));
            _ = payload.setDynamicLabel(&s, dims.LABEL_RUNNER, runner_id);
            _ = payload.addInternedLabel(&s, dims.LABEL_OUTCOME, outcome);
            st.emit(s);
        }
    }
}

fn appendSlotFamilies(st: *StreamState, slot: mr.SlotView) void {
    appendCounterCells(st, slot.runner_id, slot.failures, slot.executions);
    // null = never seen; the retired renderer skipped the gauge for such slots.
    if (slot.last_seen_seconds) |age_s| {
        var s = payload.newSample(.runner_last_seen_seconds, age_s);
        _ = payload.setDynamicLabel(&s, dims.LABEL_RUNNER, slot.runner_id);
        st.emit(s);
    }
    // A live runner's lease level is a fact even at zero — absence of this
    // gauge means the runner (or the exporter) is gone, never "no leases".
    // Transient sub-zero reads from the best-effort decrement clamp to zero.
    var leases = payload.newSample(.runner_active_leases, @max(slot.active_leases, 0));
    _ = payload.setDynamicLabel(&leases, dims.LABEL_RUNNER, slot.runner_id);
    st.emit(leases);
}
