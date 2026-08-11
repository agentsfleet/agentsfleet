//! Black-box tests for metrics_runner — drive the public push API, assert on
//! the exported OTLP window (the streamed per-runner appender rides the same
//! flush envelope as every other family). No access to the internal slot table.

const std = @import("std");
const mr = @import("metrics_runner.zig");
const window = @import("otel_metrics_window_test.zig");

// pin test: literal is the contract — the operator assets query these
// spellings; the constants in metrics_runner.zig must keep matching them.
const FAILURES_FAMILY = "agentsfleet_runner_failures_total";
const FAILURES_OVERFLOW_FAMILY = "agentsfleet_runner_failures_overflow_total";
const EXECUTIONS_FAMILY = "agentsfleet_runner_executions_total";
const LAST_SEEN_FAMILY = "agentsfleet_runner_last_seen_seconds";
const ACTIVE_LEASES_FAMILY = "agentsfleet_runner_active_leases";

const RUNNER_FAMILIES = [_][]const u8{
    FAILURES_FAMILY,
    FAILURES_OVERFLOW_FAMILY,
    EXECUTIONS_FAMILY,
    LAST_SEEN_FAMILY,
    ACTIVE_LEASES_FAMILY,
};

/// Fragments for one runner-labelled series: [runner_id attr, second attr].
fn runnerAttr(buf: []u8, runner_id: []const u8) ![]const u8 {
    return window.attrFragment(buf, "runner_id", runner_id);
}

test "failures bucket by runner and reason" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.incRunnerFailure("r1", .oom_kill);
    mr.incRunnerFailure("r1", .oom_kill);
    mr.incRunnerFailure("r1", .timeout_kill);
    mr.incRunnerFailure("r2", .renewal_terminate);

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    var id_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    const r1 = try runnerAttr(&id_buf, "r1");
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, FAILURES_FAMILY, &.{ r1, try window.attrFragment(&reason_buf, "reason", "oom_kill") }));
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, FAILURES_FAMILY, &.{ r1, try window.attrFragment(&reason_buf, "reason", "timeout_kill") }));
    var id2_buf: [96]u8 = undefined;
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, FAILURES_FAMILY, &.{ try runnerAttr(&id2_buf, "r2"), try window.attrFragment(&reason_buf, "reason", "renewal_terminate") }));
}

test "absent reason exports as reason=unknown" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.incRunnerFailure("r1", null);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    var reason_buf: [96]u8 = undefined;
    const unknown = try window.attrFragment(&reason_buf, "reason", "unknown");
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, FAILURES_FAMILY, &.{unknown}));
}

test "executions split by outcome" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .fleet_error);

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    var id_buf: [96]u8 = undefined;
    var outcome_buf: [96]u8 = undefined;
    const r1 = try runnerAttr(&id_buf, "r1");
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, EXECUTIONS_FAMILY, &.{ r1, try window.attrFragment(&outcome_buf, "outcome", "processed") }));
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, EXECUTIONS_FAMILY, &.{ r1, try window.attrFragment(&outcome_buf, "outcome", "fleet_error") }));
}

test "a seen runner exports a last_seen_seconds series" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.touchRunnerSeen("r1");
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    var id_buf: [96]u8 = undefined;
    try window.expectFamilyWith(body, LAST_SEEN_FAMILY, &.{try runnerAttr(&id_buf, "r1")});
}

test "active_leases tracks grant then release" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.incRunnerActiveLeases("r1");
    mr.incRunnerActiveLeases("r1");
    var id_buf: [96]u8 = undefined;
    const r1 = try runnerAttr(&id_buf, "r1");
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, ACTIVE_LEASES_FAMILY, &.{r1}));

    mr.decRunnerActiveLeases("r1");
    const body2 = try window.flushWindowJson(alloc);
    defer alloc.free(body2);
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body2, ACTIVE_LEASES_FAMILY, &.{r1}));
}

test "active_leases clamps below zero and exports no negative series" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.decRunnerActiveLeases("r1"); // release with no prior grant (post-restart report)
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    // The transient sub-zero clamps to zero — a live runner's lease level is a
    // fact even at zero (absence would read as "runner gone"), so the gauge
    // exports 0, never a negative level.
    var id_buf: [96]u8 = undefined;
    const r1 = try runnerAttr(&id_buf, "r1");
    try std.testing.expectEqual(@as(i64, 0), try window.familyValueWith(body, ACTIVE_LEASES_FAMILY, &.{r1}));
    try window.expectNoFamilyWith(body, ACTIVE_LEASES_FAMILY, &.{"\"asInt\":\"-"});
}

test "no runner family is exported before any runner activity" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    for (RUNNER_FAMILIES) |family| {
        try window.expectNoFamilyWith(body, family, &.{});
    }
}

test "same runner dedupes to one slot" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mr.incRunnerFailure("r-dedup", .policy_deny);
    mr.incRunnerFailure("r-dedup", .policy_deny);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    var id_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    const fragments = [_][]const u8{ try runnerAttr(&id_buf, "r-dedup"), try window.attrFragment(&reason_buf, "reason", "policy_deny") };
    // A duplicate slot claim would split the counter across TWO identically
    // labelled series; exactly one, valued 2, is the dedupe proof.
    try std.testing.expectEqual(@as(usize, 1), try window.countFamilyWith(body, FAILURES_FAMILY, &fragments));
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, FAILURES_FAMILY, &fragments));
}

// Dimension 3.4 — a known runner keeps its identity label on the wire; one
// driven past the slot capacity lands in the shared overflow family with its
// count preserved (the per-reason breakdown stays in the durable event row;
// the exported overflow family is the deliberate aggregate).
test "test_runner_families_carry_identity_and_overflow" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    var idbuf: [mr.MAX_SLOTS / 100]u8 = undefined; // ample for "runner-<n>"
    var i: usize = 0;
    while (i < mr.MAX_SLOTS) : (i += 1) {
        const id = try std.fmt.bufPrint(&idbuf, "runner-{d}", .{i});
        mr.incRunnerFailure(id, .timeout_kill);
    }
    mr.incRunnerFailure("one-too-many", .oom_kill); // overflow

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // A known runner keeps its identity label.
    var id_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, FAILURES_FAMILY, &.{
        try runnerAttr(&id_buf, "runner-0"),
        try window.attrFragment(&reason_buf, "reason", "timeout_kill"),
    }));
    // The overflowed runner's count lands in the shared bucket, preserved.
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, FAILURES_OVERFLOW_FAMILY, &.{}));
    // And it never minted a labelled series of its own.
    var over_buf: [96]u8 = undefined;
    try window.expectNoFamilyWith(body, FAILURES_FAMILY, &.{try runnerAttr(&over_buf, "one-too-many")});
}

// ── Slot resolution under contention (saturation policy, no duplicates) ─────
//
// Why resolveSlot never advances past a slot without ruling it out: a slot
// observed free can be claimed by another thread before our own compare-and-swap
// lands, and that winner may have claimed it FOR OUR KEY — probing on from a
// lost claim is precisely how one runner_id ends up owning two identically-
// labelled series, its counter split across them, when N threads first touch it
// at once. The claim barrier below exists because that window is nanoseconds
// wide on an idle machine: without parking every contender inside it, the storm
// sails through one thread at a time and the invariant is only exercised when
// the scheduler happens to starve the winner mid-init.

const StormThread = struct {
    const PER_THREAD: usize = 200;
    fn run(runner_id: []const u8) void {
        var i: usize = 0;
        while (i < PER_THREAD) : (i += 1) mr.incRunnerFailure(runner_id, null);
    }
};

test "metrics_runner_no_duplicate_slot_under_contention" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    const THREADS = 8;
    const TOTAL = THREADS * StormThread.PER_THREAD;

    // Park every thread's first claim inside the claim window until the whole
    // storm is in it (see the section comment above).
    mr.setClaimBarrierForTest(THREADS);
    defer mr.setClaimBarrierForTest(0);

    var threads: [THREADS]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, StormThread.run, .{"contended-runner"});
    for (&threads) |*t| t.join();

    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);

    // The no-duplicate proof: a duplicate slot claim for one runner_id would
    // split its counter across TWO identically-labelled series. Exactly one is
    // the invariant — asserted directly rather than inferred from the total.
    var id_buf: [96]u8 = undefined;
    var reason_buf: [96]u8 = undefined;
    const own_fragments = [_][]const u8{
        try runnerAttr(&id_buf, "contended-runner"),
        try window.attrFragment(&reason_buf, "reason", "unknown"),
    };
    try std.testing.expectEqual(@as(usize, 1), try window.countFamilyWith(body, FAILURES_FAMILY, &own_fragments));

    // Conservation: every increment is accounted for, never lost. A record whose
    // slot was still mid-init past the spin cap is DROPPED to the overflow sink
    // by the saturation policy (never probed forward into a duplicate slot), so
    // under CPU contention the split between the two series is legitimately
    // nondeterministic — only the sum is invariant. Asserting the full total on
    // the runner's own series would be asserting that the saturation policy
    // never fires, which is a load-dependent flake, not a correctness property.
    const own = try window.familyValueWith(body, FAILURES_FAMILY, &own_fragments);
    const overflow = window.familyValueWith(body, FAILURES_OVERFLOW_FAMILY, &.{}) catch |err| switch (err) {
        error.SeriesNotFound => @as(i64, 0), // no saturation drops this run
        else => return err,
    };
    try std.testing.expectEqual(@as(i64, TOTAL), own + overflow);
}
