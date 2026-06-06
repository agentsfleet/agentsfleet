//! Per-runner Prometheus metrics, pushed in on the runner verbs and rendered
//! in-memory on /metrics (no database on the scrape path).
//!
//! A fixed-capacity hash slot table keyed on runner_id holds, per runner:
//!   - failures by FailureClass (+ an `unknown` bucket)   [report]
//!   - executions by Outcome                              [report]
//!   - last-seen wall-clock stamp (liveness)              [report + heartbeat]
//!   - currently-held lease count (gauge)                 [lease grant / report]
//!
//! No allocator at runtime — compile-time capacity. Counter overflow past the
//! table routes to runner_id="_other" (reason/outcome preserved); the per-runner
//! gauges (last-seen, active-leases) are simply not tracked for overflow runners.
//! Thread-safe: CAS slot claim, lock-free atomic counters. Mirrors
//! metrics_workspace.zig. Tests live in metrics_runner_test.zig.
//!
//! active_leases is best-effort: it is decremented on a runner's report, but a
//! lease abandoned by a dead runner expires by the clock with no report, so that
//! runner's gauge stays high until process restart. Documented in the
//! runner-observability spec; the correct fix is a background Postgres
//! refresher (deferred).

const std = @import("std");
const clock = @import("common").clock;
const contract = @import("contract");
const FailureClass = contract.execution_result.FailureClass;
const Outcome = contract.protocol.Outcome;

/// Max distinct runner_ids tracked. Overflow → `_other` (counters only).
/// pub so the test file can drive the table to its cardinality edge.
pub const MAX_SLOTS: usize = 4096;
/// Truncated runner_id length stored per slot (enough for a Prometheus label).
const ID_LEN: usize = 48;
const MS_PER_S: i64 = 1000;

const FAILURES_NAME = "zombie_runner_failures_total";
const FAILURES_HELP = "Runner-executed runs that failed, labelled by runner and failure reason.";
const FAILURES_OVERFLOW_NAME = "zombie_runner_failures_overflow_total";
const FAILURES_OVERFLOW_HELP = "Failure increments routed to _other due to runner_id cardinality overflow.";
const EXECUTIONS_NAME = "zombie_runner_executions_total";
const EXECUTIONS_HELP = "Runs a runner reported, labelled by runner and outcome.";
const LAST_SEEN_NAME = "zombie_runner_last_seen_seconds";
const LAST_SEEN_HELP = "Seconds since a runner was last seen (report or heartbeat); computed at render.";
const ACTIVE_LEASES_NAME = "zombie_runner_active_leases";
const ACTIVE_LEASES_HELP = "Leases a runner currently holds (best-effort; abandoned leases self-heal on restart).";
// Memory-capture families are GLOBAL (unlabelled): per-zombie labels would explode
// cardinality. The zombie scope rides the structured log line, never a metric label.
const MEM_CAPTURED_NAME = "zombie_memory_entries_captured_total";
const MEM_CAPTURED_HELP = "Durable memory entries persisted via the runner-plane capture push.";
const MEM_PUSH_FAIL_NAME = "zombie_memory_push_failures_total";
const MEM_PUSH_FAIL_HELP = "Memory capture pushes that failed to persist (ERR_MEM_UNAVAILABLE).";
const MEM_HYDRATION_NAME = "zombie_memory_hydration_window_entries";
const MEM_HYDRATION_HELP = "Entry count in the most recent hydration window served to a runner.";
// Prometheus exposition format strings — single-sourced (RULE UFS). The format arg
// to writer.print must be comptime; a container-level const satisfies that.
const FMT_HELP_TYPE = "# HELP {s} {s}\n# TYPE {s} {s}\n";
const FMT_HELP_TYPE_VALUE = "# HELP {s} {s}\n# TYPE {s} {s}\n{s} {d}\n";
const FMT_SERIES_2LABEL = "{s}{{{s}=\"{s}\",{s}=\"{s}\"}} {d}\n";
const FMT_SERIES_1LABEL = "{s}{{{s}=\"{s}\"}} {d}\n";
const LABEL_RUNNER = "runner_id";
const LABEL_REASON = "reason";
const LABEL_OUTCOME = "outcome";
const REASON_UNKNOWN = "unknown";
const ID_OTHER = "_other";
const TYPE_COUNTER = "counter";
const TYPE_GAUGE = "gauge";

const reason_fields = @typeInfo(FailureClass).@"enum".fields;
const N_REASONS: usize = reason_fields.len + 1;
const UNKNOWN_IDX: usize = reason_fields.len;
const REASON_LABELS: [N_REASONS][]const u8 = blk: {
    var labels: [N_REASONS][]const u8 = undefined;
    for (reason_fields, 0..) |f, i| labels[i] = f.name;
    labels[UNKNOWN_IDX] = REASON_UNKNOWN;
    break :blk labels;
};

const outcome_fields = @typeInfo(Outcome).@"enum".fields;
const N_OUTCOMES: usize = outcome_fields.len;
const OUTCOME_LABELS: [N_OUTCOMES][]const u8 = blk: {
    var labels: [N_OUTCOMES][]const u8 = undefined;
    for (outcome_fields, 0..) |f, i| labels[i] = f.name;
    break :blk labels;
};

const Counters = struct {
    failures: [N_REASONS]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** N_REASONS,
    executions: [N_OUTCOMES]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** N_OUTCOMES,
};

const Slot = struct {
    occupied: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    ready: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    runner_id: [ID_LEN]u8 = [_]u8{0} ** ID_LEN,
    runner_id_len: u8 = 0,
    hash: u64 = 0,
    counters: Counters = .{},
    /// Wall-clock ms of the last report/heartbeat; 0 = never seen.
    last_seen_ms: std.atomic.Value(i64) = std.atomic.Value(i64).init(0),
    /// Currently-held leases; may transiently read <0 under best-effort dec.
    active_leases: std.atomic.Value(i64) = std.atomic.Value(i64).init(0),
};

var g_slots: [MAX_SLOTS]Slot = [_]Slot{.{}} ** MAX_SLOTS;
/// Per-reason/outcome counts for runners that overflowed the table.
var g_overflow: Counters = .{};
/// Total failure overflow increments, surfaced as an explicit counter.
var g_overflow_total = std.atomic.Value(u64).init(0);
var g_slot_count = std.atomic.Value(u32).init(0);

/// Global memory-capture telemetry (low cardinality, no per-zombie label).
var g_memory_captured_total = std.atomic.Value(u64).init(0);
var g_memory_push_failures_total = std.atomic.Value(u64).init(0);
var g_memory_hydration_entries = std.atomic.Value(i64).init(0);

fn bucketIndex(reason: ?FailureClass) usize {
    return if (reason) |r| @intFromEnum(r) else UNKNOWN_IDX;
}

fn runnerHash(runner_id: []const u8) u64 {
    var h = std.hash.Wyhash.init(0);
    h.update(runner_id);
    return h.final();
}

fn slotMatches(slot: *const Slot, h: u64, runner_id: []const u8) bool {
    if (slot.hash != h) return false;
    const cmp = @min(runner_id.len, ID_LEN);
    if (slot.runner_id_len != cmp) return false;
    return std.mem.eql(u8, slot.runner_id[0..slot.runner_id_len], runner_id[0..cmp]);
}

fn initSlot(slot: *Slot, h: u64, runner_id: []const u8) void {
    const len: u8 = @intCast(@min(runner_id.len, ID_LEN));
    @memcpy(slot.runner_id[0..len], runner_id[0..len]);
    slot.runner_id_len = len;
    slot.hash = h;
    slot.ready.store(1, .release); // safe because: publishes the init writes above to readers loading ready with .acquire
}

/// Linear-probe to the runner's slot, claiming a fresh one on first sight.
/// Returns null only when every slot is occupied (cardinality overflow).
fn resolveSlot(runner_id: []const u8) ?*Slot {
    const h = runnerHash(runner_id);
    const start = h % MAX_SLOTS;
    var i: usize = 0;
    while (i < MAX_SLOTS) : (i += 1) {
        const idx = (start + i) % MAX_SLOTS;
        const slot = &g_slots[idx];

        const occ = slot.occupied.load(.acquire); // safe because: pairs with the cmpxchg release on claim
        if (occ == 1) {
            // Bounded spin so we never race past a mid-init slot for our own
            // key (which would claim a duplicate); the cap falls back to
            // probe-forward if an initializer was suspended mid-init.
            var spins: u32 = 0;
            const SPIN_CAP: u32 = 4096;
            while (slot.ready.load(.acquire) != 1) { // safe because: pairs with the .release store in initSlot
                if (spins >= SPIN_CAP) break;
                std.atomic.spinLoopHint();
                spins += 1;
            }
            if (slot.ready.load(.acquire) == 1 and slotMatches(slot, h, runner_id)) return slot;
            continue;
        }

        if (slot.occupied.cmpxchgStrong(0, 1, .acq_rel, .acquire)) |_| continue;
        initSlot(slot, h, runner_id);
        _ = g_slot_count.fetchAdd(1, .monotonic); // safe because: independent counter, no ordering dependency
        return slot;
    }
    return null;
}

// ── Push API (called from the runner verbs) ─────────────────────────────────

/// Record one failed run for `runner_id`, bucketed by reason (absent → unknown).
pub fn incRunnerFailure(runner_id: []const u8, reason: ?FailureClass) void {
    const idx = bucketIndex(reason);
    if (resolveSlot(runner_id)) |slot| {
        _ = slot.counters.failures[idx].fetchAdd(1, .monotonic); // safe because: independent counter
    } else {
        _ = g_overflow.failures[idx].fetchAdd(1, .monotonic); // safe because: independent counter
        _ = g_overflow_total.fetchAdd(1, .monotonic); // safe because: independent counter
    }
}

/// Record one reported run for `runner_id`, bucketed by outcome, and stamp
/// liveness. Overflow runners count under _other (no liveness stamp).
pub fn observeRunnerExecution(runner_id: []const u8, outcome: Outcome) void {
    const idx = @intFromEnum(outcome);
    if (resolveSlot(runner_id)) |slot| {
        _ = slot.counters.executions[idx].fetchAdd(1, .monotonic); // safe because: independent counter
        slot.last_seen_ms.store(clock.nowMillis(), .monotonic); // safe because: lone gauge stamp, last-writer-wins
    } else {
        _ = g_overflow.executions[idx].fetchAdd(1, .monotonic); // safe because: independent counter
    }
}

/// Stamp liveness for `runner_id` (heartbeat). No-op for overflow runners.
pub fn touchRunnerSeen(runner_id: []const u8) void {
    if (resolveSlot(runner_id)) |slot| {
        slot.last_seen_ms.store(clock.nowMillis(), .monotonic); // safe because: lone gauge stamp, last-writer-wins
    }
}

/// A lease was granted to `runner_id` (gauge +1). No-op for overflow runners.
pub fn incRunnerActiveLeases(runner_id: []const u8) void {
    if (resolveSlot(runner_id)) |slot| {
        _ = slot.active_leases.fetchAdd(1, .monotonic); // safe because: independent gauge
    }
}

/// A lease held by `runner_id` was released via report (gauge -1). Best-effort:
/// a lease abandoned without a report is never decremented (see module note).
pub fn decRunnerActiveLeases(runner_id: []const u8) void {
    if (resolveSlot(runner_id)) |slot| {
        _ = slot.active_leases.fetchSub(1, .monotonic); // safe because: independent gauge; render clamps <0 to 0
    }
}

/// `n` memory entries were persisted by a capture push. Global counter (no label).
pub fn incMemoryCaptured(n: usize) void {
    if (n == 0) return;
    _ = g_memory_captured_total.fetchAdd(@intCast(n), .monotonic); // safe because: independent counter
}

/// A memory capture push failed to persist (ERR_MEM_UNAVAILABLE). Global counter.
pub fn incMemoryPushFailure() void {
    _ = g_memory_push_failures_total.fetchAdd(1, .monotonic); // safe because: independent counter
}

/// Record the entry count of the most recent hydration window (gauge, last-writer-wins).
pub fn setMemoryHydrationEntries(n: usize) void {
    g_memory_hydration_entries.store(@intCast(n), .monotonic); // safe because: lone gauge, last-writer-wins
}

// ── Prometheus rendering (in-memory; called by metrics_render) ───────────────

fn renderFailureSeries(writer: anytype, runner: []const u8, c: *const Counters) !void {
    for (&c.failures, 0..) |*v, idx| {
        const val = v.load(.acquire); // safe because: pairs with the fetchAdd in incRunnerFailure
        if (val == 0) continue;
        try writer.print(FMT_SERIES_2LABEL, .{ FAILURES_NAME, LABEL_RUNNER, runner, LABEL_REASON, REASON_LABELS[idx], val });
    }
}

fn renderExecutionSeries(writer: anytype, runner: []const u8, c: *const Counters) !void {
    for (&c.executions, 0..) |*v, idx| {
        const val = v.load(.acquire); // safe because: pairs with the fetchAdd in observeRunnerExecution
        if (val == 0) continue;
        try writer.print(FMT_SERIES_2LABEL, .{ EXECUTIONS_NAME, LABEL_RUNNER, runner, LABEL_OUTCOME, OUTCOME_LABELS[idx], val });
    }
}

fn renderCounterFamilies(writer: anytype) !void {
    try writer.print(FMT_HELP_TYPE, .{ FAILURES_NAME, FAILURES_HELP, FAILURES_NAME, TYPE_COUNTER });
    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        try renderFailureSeries(writer, slot.runner_id[0..slot.runner_id_len], &slot.counters);
    }
    try renderFailureSeries(writer, ID_OTHER, &g_overflow);
    try writer.print(FMT_HELP_TYPE_VALUE, .{ FAILURES_OVERFLOW_NAME, FAILURES_OVERFLOW_HELP, FAILURES_OVERFLOW_NAME, TYPE_COUNTER, FAILURES_OVERFLOW_NAME, g_overflow_total.load(.acquire) });

    try writer.print(FMT_HELP_TYPE, .{ EXECUTIONS_NAME, EXECUTIONS_HELP, EXECUTIONS_NAME, TYPE_COUNTER });
    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        try renderExecutionSeries(writer, slot.runner_id[0..slot.runner_id_len], &slot.counters);
    }
    try renderExecutionSeries(writer, ID_OTHER, &g_overflow);
}

fn renderGaugeFamilies(writer: anytype) !void {
    const now_ms = clock.nowMillis();
    try writer.print(FMT_HELP_TYPE, .{ LAST_SEEN_NAME, LAST_SEEN_HELP, LAST_SEEN_NAME, TYPE_GAUGE });
    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        const seen = slot.last_seen_ms.load(.acquire); // safe because: pairs with the store in observe/touch
        if (seen == 0) continue;
        const age_s = @divFloor(@max(0, now_ms - seen), MS_PER_S);
        try writer.print(FMT_SERIES_1LABEL, .{ LAST_SEEN_NAME, LABEL_RUNNER, slot.runner_id[0..slot.runner_id_len], age_s });
    }

    try writer.print(FMT_HELP_TYPE, .{ ACTIVE_LEASES_NAME, ACTIVE_LEASES_HELP, ACTIVE_LEASES_NAME, TYPE_GAUGE });
    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        const held = @max(0, slot.active_leases.load(.acquire)); // safe because: best-effort gauge; clamp transient <0
        if (held == 0) continue;
        try writer.print(FMT_SERIES_1LABEL, .{ ACTIVE_LEASES_NAME, LABEL_RUNNER, slot.runner_id[0..slot.runner_id_len], held });
    }
}

/// Global memory-capture families (unlabelled, low cardinality). Counters render
/// even at 0 once any runner/memory activity exists; the gauge clamps transient <0.
fn renderMemoryFamilies(writer: anytype) !void {
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_CAPTURED_NAME, MEM_CAPTURED_HELP, MEM_CAPTURED_NAME, TYPE_COUNTER, MEM_CAPTURED_NAME, g_memory_captured_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_PUSH_FAIL_NAME, MEM_PUSH_FAIL_HELP, MEM_PUSH_FAIL_NAME, TYPE_COUNTER, MEM_PUSH_FAIL_NAME, g_memory_push_failures_total.load(.acquire) });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_HYDRATION_NAME, MEM_HYDRATION_HELP, MEM_HYDRATION_NAME, TYPE_GAUGE, MEM_HYDRATION_NAME, @max(0, g_memory_hydration_entries.load(.acquire)) });
}

/// Render every per-runner family. Emits nothing until a runner has been seen or a
/// memory capture has occurred.
pub fn renderPrometheus(writer: anytype) !void {
    const mem_active = g_memory_captured_total.load(.acquire) != 0 or g_memory_push_failures_total.load(.acquire) != 0;
    if (g_slot_count.load(.acquire) == 0 and g_overflow_total.load(.acquire) == 0 and !mem_active) return;
    try renderCounterFamilies(writer);
    try renderGaugeFamilies(writer);
    try renderMemoryFamilies(writer);
}

// Test-only reset, consumed by metrics_runner_test.zig.
pub fn resetForTest() void {
    g_slots = [_]Slot{.{}} ** MAX_SLOTS;
    g_overflow = .{};
    g_overflow_total.store(0, .release);
    g_slot_count.store(0, .release);
    g_memory_captured_total.store(0, .release);
    g_memory_push_failures_total.store(0, .release);
    g_memory_hydration_entries.store(0, .release);
}
