//! Per-runner failure Prometheus counter.
//!
//! Fixed-capacity hash slot table keyed on runner_id, each slot holding one
//! counter per failure reason (the runner's `FailureClass` plus an `unknown`
//! bucket for a failure whose reason the runner did not report). No allocator at
//! runtime — compile-time capacity. Overflow routes to `_other`.
//!
//! Thread-safe: slot claim uses CAS on first write, atomic loads afterwards.
//! Counter increments are lock-free atomic fetchAdd. Mirrors metrics_workspace.zig
//! (single-keyed here; the second dimension is a fixed per-reason array, not a
//! hashed label).

const std = @import("std");
const FailureClass = @import("contract").execution_result.FailureClass;

/// Max distinct runner_ids tracked. Overflow → `_other`.
const MAX_SLOTS: usize = 4096;
/// Truncated runner_id length stored per slot (enough for a Prometheus label).
const ID_LEN: usize = 48;

const METRIC_NAME = "zombie_runner_failures_total";
const METRIC_HELP = "Runner-executed runs that failed, labelled by runner and failure reason.";
const OVERFLOW_NAME = "zombie_runner_failures_overflow_total";
const OVERFLOW_HELP = "Failure increments routed to _other due to runner_id cardinality overflow.";
const LABEL_RUNNER = "runner_id";
const LABEL_REASON = "reason";
const REASON_UNKNOWN = "unknown";
const ID_OTHER = "_other";

const reason_fields = @typeInfo(FailureClass).@"enum".fields;
/// One counter per FailureClass variant, plus a trailing `unknown` bucket.
const N_BUCKETS: usize = reason_fields.len + 1;
const UNKNOWN_IDX: usize = reason_fields.len;

/// Label string for each bucket index — variant tag names verbatim (RULE UFS:
/// single-sourced from the enum), with `unknown` in the trailing slot.
const REASON_LABELS: [N_BUCKETS][]const u8 = blk: {
    var labels: [N_BUCKETS][]const u8 = undefined;
    for (reason_fields, 0..) |f, i| labels[i] = f.name;
    labels[UNKNOWN_IDX] = REASON_UNKNOWN;
    break :blk labels;
};

const Counters = struct {
    failures: [N_BUCKETS]std.atomic.Value(u64) = [_]std.atomic.Value(u64){std.atomic.Value(u64).init(0)} ** N_BUCKETS,
};

const Slot = struct {
    occupied: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    ready: std.atomic.Value(u8) = std.atomic.Value(u8).init(0),
    runner_id: [ID_LEN]u8 = [_]u8{0} ** ID_LEN,
    runner_id_len: u8 = 0,
    hash: u64 = 0,
    counters: Counters = .{},
};

var g_slots: [MAX_SLOTS]Slot = [_]Slot{.{}} ** MAX_SLOTS;
/// Per-reason counts for runners that overflowed the table — rendered under
/// runner_id="_other" so the reason dimension survives a cardinality spill.
var g_overflow: Counters = .{};
/// Total overflow increments, surfaced as an explicit counter so a spill is
/// visible without summing the _other series.
var g_overflow_total = std.atomic.Value(u64).init(0);
var g_slot_count = std.atomic.Value(u32).init(0);

/// Map a reported reason to its bucket index; absent → the `unknown` bucket.
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
    slot.ready.store(1, .release); // safe because: publishes the init writes above to readers that load ready with .acquire
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
            // key (which would claim a duplicate). The cap prevents a wedge if
            // an initializer was suspended between CAS and ready-publish — we
            // fall back to probing forward, accepting one duplicate slot for
            // that pathological case rather than a process-wide stall.
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

/// Record one failed run for `runner_id`, bucketed by reason. A failure whose
/// reason the runner did not report lands in the `unknown` bucket. Overflow
/// past slot capacity is counted under `_other`.
pub fn incRunnerFailure(runner_id: []const u8, reason: ?FailureClass) void {
    const idx = bucketIndex(reason);
    if (resolveSlot(runner_id)) |slot| {
        _ = slot.counters.failures[idx].fetchAdd(1, .monotonic); // safe because: independent counter, no ordering dependency
    } else {
        // Cardinality overflow: bucket under runner_id="_other" preserving the
        // reason, and bump the explicit total so the spill is visible.
        _ = g_overflow.failures[idx].fetchAdd(1, .monotonic); // safe because: independent counter, no ordering dependency
        _ = g_overflow_total.fetchAdd(1, .monotonic); // safe because: independent counter, no ordering dependency
    }
}

/// Emit one (runner_id, reason) series per non-zero counter in `counters`.
fn renderSeries(writer: anytype, runner: []const u8, counters: *const Counters) !void {
    for (&counters.failures, 0..) |*c, idx| {
        const val = c.load(.acquire); // safe because: pairs with the fetchAdd in incRunnerFailure
        if (val == 0) continue;
        try writer.print("{s}{{{s}=\"{s}\",{s}=\"{s}\"}} {d}\n", .{
            METRIC_NAME, LABEL_RUNNER, runner, LABEL_REASON, REASON_LABELS[idx], val,
        });
    }
}

/// Emit the failures family as a single HELP/TYPE block followed by one series
/// per (runner_id, reason) with a non-zero count, plus the overflow counter.
pub fn renderPrometheus(writer: anytype) !void {
    const count = g_slot_count.load(.acquire); // safe because: pairs with the fetchAdd on slot claim
    if (count == 0 and g_overflow_total.load(.acquire) == 0) return;

    try writer.print("# HELP {s} {s}\n", .{ METRIC_NAME, METRIC_HELP });
    try writer.print("# TYPE {s} counter\n", .{METRIC_NAME});
    for (&g_slots) |*slot| {
        if (slot.occupied.load(.acquire) != 1 or slot.ready.load(.acquire) != 1) continue;
        try renderSeries(writer, slot.runner_id[0..slot.runner_id_len], &slot.counters);
    }
    try renderSeries(writer, ID_OTHER, &g_overflow); // cardinality spill, reason preserved

    try writer.print("# HELP {s} {s}\n", .{ OVERFLOW_NAME, OVERFLOW_HELP });
    try writer.print("# TYPE {s} counter\n", .{OVERFLOW_NAME});
    try writer.print("{s} {d}\n", .{ OVERFLOW_NAME, g_overflow_total.load(.acquire) });
}

// ── Tests ─────────────────────────────────────────────────────────────────

fn resetForTest() void {
    g_slots = [_]Slot{.{}} ** MAX_SLOTS;
    g_overflow = .{};
    g_overflow_total.store(0, .release);
    g_slot_count.store(0, .release);
}

test "incRunnerFailure buckets by reason per runner" {
    resetForTest();
    incRunnerFailure("r1", .oom_kill);
    incRunnerFailure("r1", .oom_kill);
    incRunnerFailure("r1", .timeout_kill);
    incRunnerFailure("r2", .renewal_terminate);

    const r1 = resolveSlot("r1").?;
    try std.testing.expectEqual(@as(u64, 2), r1.counters.failures[@intFromEnum(FailureClass.oom_kill)].load(.acquire));
    try std.testing.expectEqual(@as(u64, 1), r1.counters.failures[@intFromEnum(FailureClass.timeout_kill)].load(.acquire));
    const r2 = resolveSlot("r2").?;
    try std.testing.expectEqual(@as(u64, 1), r2.counters.failures[@intFromEnum(FailureClass.renewal_terminate)].load(.acquire));
    try std.testing.expectEqual(@as(u32, 2), g_slot_count.load(.acquire));
}

test "absent reason lands in the unknown bucket" {
    resetForTest();
    incRunnerFailure("r1", null);
    const r1 = resolveSlot("r1").?;
    try std.testing.expectEqual(@as(u64, 1), r1.counters.failures[UNKNOWN_IDX].load(.acquire));
}

test "renderPrometheus emits runner_id and reason labels" {
    resetForTest();
    incRunnerFailure("runner-42", .executor_crash);

    var buf: [4096]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try renderPrometheus(fbs.writer());
    const out = fbs.getWritten();

    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, METRIC_NAME));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "runner_id=\"runner-42\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, "reason=\"executor_crash\""));
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, " 1\n"));
    // The overflow family is always emitted alongside the data (here at 0).
    try std.testing.expect(std.mem.containsAtLeast(u8, out, 1, OVERFLOW_NAME));
}

test "unknown reason renders as reason=unknown" {
    resetForTest();
    incRunnerFailure("r1", null);

    var buf: [2048]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try renderPrometheus(fbs.writer());
    try std.testing.expect(std.mem.containsAtLeast(u8, fbs.getWritten(), 1, "reason=\"unknown\""));
}

test "render is empty before any failure" {
    resetForTest();
    var buf: [256]u8 = undefined;
    var fbs = std.io.fixedBufferStream(&buf);
    try renderPrometheus(fbs.writer());
    try std.testing.expectEqual(@as(usize, 0), fbs.getWritten().len);
}

test "same runner resolves to the same slot" {
    resetForTest();
    incRunnerFailure("r-dedup", .policy_deny);
    incRunnerFailure("r-dedup", .policy_deny);
    try std.testing.expectEqual(@as(u32, 1), g_slot_count.load(.acquire));
    try std.testing.expectEqual(
        @as(u64, 2),
        resolveSlot("r-dedup").?.counters.failures[@intFromEnum(FailureClass.policy_deny)].load(.acquire),
    );
}

test "overflow past capacity routes to _other without crashing" {
    resetForTest();
    // Fill every slot with distinct runner_ids, then one more must overflow.
    var i: usize = 0;
    var idbuf: [ID_LEN]u8 = undefined;
    while (i < MAX_SLOTS) : (i += 1) {
        const id = std.fmt.bufPrint(&idbuf, "runner-{d}", .{i}) catch unreachable;
        incRunnerFailure(id, .timeout_kill);
    }
    try std.testing.expectEqual(@as(u64, 0), g_overflow_total.load(.acquire));
    incRunnerFailure("one-too-many", .timeout_kill); // 4097th distinct runner → _other
    try std.testing.expectEqual(@as(u64, 1), g_overflow_total.load(.acquire));
    // Reason is preserved in the _other bucket, not flattened away.
    try std.testing.expectEqual(@as(u64, 1), g_overflow.failures[@intFromEnum(FailureClass.timeout_kill)].load(.acquire));
}
