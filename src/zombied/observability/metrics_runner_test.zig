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
    try std.testing.expect(contains(out, "zombie_runner_failures_total{runner_id=\"r1\",reason=\"oom_kill\"} 2"));
    try std.testing.expect(contains(out, "zombie_runner_failures_total{runner_id=\"r1\",reason=\"timeout_kill\"} 1"));
    try std.testing.expect(contains(out, "zombie_runner_failures_total{runner_id=\"r2\",reason=\"renewal_terminate\"} 1"));
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
    mr.observeRunnerExecution("r1", .agent_error);

    var buf: [8192]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "zombie_runner_executions_total{runner_id=\"r1\",outcome=\"processed\"} 3"));
    try std.testing.expect(contains(out, "zombie_runner_executions_total{runner_id=\"r1\",outcome=\"agent_error\"} 1"));
}

test "a seen runner renders a last_seen_seconds series" {
    mr.resetForTest();
    mr.touchRunnerSeen("r1");
    var buf: [4096]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "zombie_runner_last_seen_seconds{runner_id=\"r1\"}"));
}

test "active_leases tracks grant then release" {
    mr.resetForTest();
    mr.incRunnerActiveLeases("r1");
    mr.incRunnerActiveLeases("r1");
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "zombie_runner_active_leases{runner_id=\"r1\"} 2"));

    mr.decRunnerActiveLeases("r1");
    try std.testing.expect(contains(try render(&buf), "zombie_runner_active_leases{runner_id=\"r1\"} 1"));
}

test "active_leases clamps below zero and emits no negative series" {
    mr.resetForTest();
    mr.decRunnerActiveLeases("r1"); // release with no prior grant (post-restart report)
    var buf: [4096]u8 = undefined;
    const out = try render(&buf);
    // Clamped to 0, and a 0 gauge is omitted — so no active_leases line for r1.
    try std.testing.expect(!contains(out, "zombie_runner_active_leases{runner_id=\"r1\"}"));
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
    try std.testing.expect(contains(text, "zombie_runner_failures_total{runner_id=\"_other\",reason=\"oom_kill\"} 1"));
    try std.testing.expect(contains(text, "zombie_runner_failures_overflow_total 1"));
}

test "memory capture counter accumulates and renders" {
    mr.resetForTest();
    mr.incMemoryCaptured(3);
    mr.incMemoryCaptured(2);
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "zombie_memory_entries_captured_total 5"));
}

test "memory push-failure counter renders" {
    mr.resetForTest();
    mr.incMemoryPushFailure();
    mr.incMemoryPushFailure();
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "zombie_memory_push_failures_total 2"));
}

test "hydration window gauge reflects the last set size" {
    mr.resetForTest();
    mr.incMemoryCaptured(1); // a counter must be non-zero for the family to render
    mr.setMemoryHydrationEntries(7);
    var buf: [4096]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "zombie_memory_hydration_window_entries 7"));
}

test "incMemoryCaptured(0) is a no-op and does not force render" {
    mr.resetForTest();
    mr.incMemoryCaptured(0);
    var buf: [256]u8 = undefined;
    try std.testing.expectEqual(@as(usize, 0), (try render(&buf)).len);
}
