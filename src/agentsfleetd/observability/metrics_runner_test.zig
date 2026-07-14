//! Black-box tests for metrics_runner — drive the public push API, assert on the
//! rendered Prometheus exposition. No access to the internal slot table.

const std = @import("std");
const mr = @import("metrics_runner.zig");

/// Render into a caller buffer and return the written slice. Sized for the
/// handful of runners each test creates (overflow test renders its own way).
fn render(buf: []u8) ![]const u8 {
    var w = std.Io.Writer.fixed(buf);
    try mr.renderPrometheus(&w);
    return w.buffered();
}

fn contains(haystack: []const u8, needle: []const u8) bool {
    return std.mem.containsAtLeast(u8, haystack, 1, needle);
}

test "failures bucket by runner and reason" {
    mr.resetForTest();
    mr.incRunnerFailure("r1", .oom_kill);
    mr.incRunnerFailure("r1", .oom_kill);
    mr.incRunnerFailure("r1", .timeout_kill);
    mr.incRunnerFailure("r2", .renewal_terminate);

    var buf: [8192]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "agentsfleet_runner_failures_total{runner_id=\"r1\",reason=\"oom_kill\"} 2"));
    try std.testing.expect(contains(out, "agentsfleet_runner_failures_total{runner_id=\"r1\",reason=\"timeout_kill\"} 1"));
    try std.testing.expect(contains(out, "agentsfleet_runner_failures_total{runner_id=\"r2\",reason=\"renewal_terminate\"} 1"));
}

test "absent reason renders as reason=unknown" {
    mr.resetForTest();
    mr.incRunnerFailure("r1", null);
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "reason=\"unknown\"} 1"));
}

test "executions split by outcome" {
    mr.resetForTest();
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .processed);
    mr.observeRunnerExecution("r1", .fleet_error);

    var buf: [8192]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "agentsfleet_runner_executions_total{runner_id=\"r1\",outcome=\"processed\"} 3"));
    try std.testing.expect(contains(out, "agentsfleet_runner_executions_total{runner_id=\"r1\",outcome=\"fleet_error\"} 1"));
}

test "a seen runner renders a last_seen_seconds series" {
    mr.resetForTest();
    mr.touchRunnerSeen("r1");
    var buf: [4096]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "agentsfleet_runner_last_seen_seconds{runner_id=\"r1\"}"));
}

test "active_leases tracks grant then release" {
    mr.resetForTest();
    mr.incRunnerActiveLeases("r1");
    mr.incRunnerActiveLeases("r1");
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "agentsfleet_runner_active_leases{runner_id=\"r1\"} 2"));

    mr.decRunnerActiveLeases("r1");
    try std.testing.expect(contains(try render(&buf), "agentsfleet_runner_active_leases{runner_id=\"r1\"} 1"));
}

test "active_leases clamps below zero and emits no negative series" {
    mr.resetForTest();
    mr.decRunnerActiveLeases("r1"); // release with no prior grant (post-restart report)
    var buf: [4096]u8 = undefined;
    const out = try render(&buf);
    // Clamped to 0, and a 0 gauge is omitted — so no active_leases line for r1.
    try std.testing.expect(!contains(out, "agentsfleet_runner_active_leases{runner_id=\"r1\"}"));
}

test "render is empty before any runner activity" {
    mr.resetForTest();
    var buf: [256]u8 = undefined;
    try std.testing.expectEqual(@as(usize, 0), (try render(&buf)).len);
}

test "same runner dedupes to one slot" {
    mr.resetForTest();
    mr.incRunnerFailure("r-dedup", .policy_deny);
    mr.incRunnerFailure("r-dedup", .policy_deny);
    var buf: [4096]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "reason=\"policy_deny\"} 2"));
}

test "cardinality overflow routes to _other with the reason preserved" {
    mr.resetForTest();
    var idbuf: [mr.MAX_SLOTS / 100]u8 = undefined; // ample for "runner-<n>"
    var i: usize = 0;
    while (i < mr.MAX_SLOTS) : (i += 1) {
        const id = try std.fmt.bufPrint(&idbuf, "runner-{d}", .{i});
        mr.incRunnerFailure(id, .timeout_kill);
    }
    mr.incRunnerFailure("one-too-many", .oom_kill); // overflow

    var aw: std.Io.Writer.Allocating = .init(std.testing.allocator);
    defer aw.deinit();
    try mr.renderPrometheus(&aw.writer);
    const text = aw.written();
    try std.testing.expect(contains(text, "agentsfleet_runner_failures_total{runner_id=\"_other\",reason=\"oom_kill\"} 1"));
    try std.testing.expect(contains(text, "agentsfleet_runner_failures_overflow_total 1"));
}

// The fleet_memory_* family tests moved to metrics_memory_test.zig with the
// module split; renderPrometheus still composes those families after the
// runner ones, pinned there through this same render entry point.

// ── Slot resolution under contention (saturation policy, no duplicates) ─────
//
// Why resolveSlot never advances past a slot without ruling it out: a slot
// observed free can be claimed by another thread before our own compare-and-swap
// lands, and that winner may have claimed it FOR OUR KEY — probing on from a
// lost claim is precisely how one runner_id ends up owning two identically-
// labelled Prometheus series, its counter split across them, when N threads
// first touch it at once. The claim barrier below exists because that window is
// nanoseconds wide on an idle machine: without parking every contender inside
// it, the storm sails through one thread at a time and the invariant is only
// exercised when the scheduler happens to starve the winner mid-init.

const StormThread = struct {
    const PER_THREAD: usize = 200;
    fn run(runner_id: []const u8) void {
        var i: usize = 0;
        while (i < PER_THREAD) : (i += 1) mr.incRunnerFailure(runner_id, null);
    }
};

/// Occurrences of `needle` in `haystack` — a second hit for one runner_id's
/// series IS the duplicate-slot defect.
fn countOccurrences(haystack: []const u8, needle: []const u8) usize {
    return std.mem.count(u8, haystack, needle);
}

/// Value of the exposition line starting with `prefix` (0 when absent — a
/// series with no increments is never rendered).
fn seriesValue(out: []const u8, prefix: []const u8) !u64 {
    const start = std.mem.indexOf(u8, out, prefix) orelse return 0;
    const rest = out[start + prefix.len ..];
    const end = std.mem.indexOfScalar(u8, rest, '\n') orelse rest.len;
    return std.fmt.parseInt(u64, std.mem.trim(u8, rest[0..end], " "), 10);
}

test "metrics_runner_no_duplicate_slot_under_contention" {
    mr.resetForTest();
    const THREADS = 8;
    const TOTAL = THREADS * StormThread.PER_THREAD;

    // Park every thread's first claim inside the claim window until the whole
    // storm is in it (see the section comment above).
    mr.setClaimBarrierForTest(THREADS);
    defer mr.setClaimBarrierForTest(0);

    var threads: [THREADS]std.Thread = undefined;
    for (&threads) |*t| t.* = try std.Thread.spawn(.{}, StormThread.run, .{"contended-runner"});
    for (&threads) |*t| t.join();

    var buf: [8192]u8 = undefined;
    const out = try render(&buf);

    // The no-duplicate proof: a duplicate slot claim for one runner_id would
    // split its counter across TWO identically-labelled series. Exactly one is
    // the invariant — asserted directly rather than inferred from the total.
    const own_prefix = "agentsfleet_runner_failures_total{runner_id=\"contended-runner\",reason=\"unknown\"} ";
    try std.testing.expectEqual(@as(usize, 1), countOccurrences(out, own_prefix));

    // Conservation: every increment is accounted for, never lost. A record whose
    // slot was still mid-init past the spin cap is DROPPED to `_other` by the
    // saturation policy (never probed forward into a duplicate slot), so under
    // CPU contention the split between the two series is legitimately
    // nondeterministic — only the sum is invariant. Asserting the full total on
    // the runner's own series would be asserting that the saturation policy
    // never fires, which is a load-dependent flake, not a correctness property.
    const own = try seriesValue(out, own_prefix);
    const other = try seriesValue(out, "agentsfleet_runner_failures_total{runner_id=\"_other\",reason=\"unknown\"} ");
    try std.testing.expectEqual(@as(u64, TOTAL), own + other);
}
