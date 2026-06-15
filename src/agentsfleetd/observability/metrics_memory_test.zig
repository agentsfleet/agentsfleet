//! Black-box tests for metrics_memory — drive the public push API, assert on
//! exact snapshot deltas and the rendered Prometheus exposition. The memory
//! families render through metrics_runner.renderPrometheus (the /metrics
//! composition), so the capture/push/hydration tests that moved here from
//! metrics_runner_test.zig keep asserting through that same entry point —
//! pinning that the module split changed no rendered name, type, or value.

const std = @import("std");
const mm = @import("metrics_memory.zig");
const mr = @import("metrics_runner.zig");

/// Render the full runner+memory exposition into a caller buffer.
fn render(buf: []u8) ![]const u8 {
    var w = std.Io.Writer.fixed(buf);
    try mr.renderPrometheus(&w);
    return w.buffered();
}

fn contains(haystack: []const u8, needle: []const u8) bool {
    return std.mem.containsAtLeast(u8, haystack, 1, needle);
}

// ── Moved from metrics_runner_test.zig (pre-split behaviour pinned) ─────────

test "memory capture counter accumulates and renders" {
    mr.resetForTest();
    mm.incMemoryCaptured(3);
    mm.incMemoryCaptured(2);
    var buf: [8192]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "agent_memory_entries_captured_total 5"));
}

test "memory push-failure counter renders" {
    mr.resetForTest();
    mm.incMemoryPushFailure();
    mm.incMemoryPushFailure();
    var buf: [8192]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "agent_memory_push_failures_total 2"));
}

test "hydration window gauge reflects the last set size" {
    mr.resetForTest();
    mm.incMemoryCaptured(1); // a counter must be non-zero for the family to render
    mm.setMemoryHydrationEntries(7);
    var buf: [8192]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "agent_memory_hydration_window_entries 7"));
}

test "incMemoryCaptured(0) is a no-op and does not force render" {
    mr.resetForTest();
    mm.incMemoryCaptured(0);
    var buf: [256]u8 = undefined;
    try std.testing.expectEqual(@as(usize, 0), (try render(&buf)).len);
}

// ── Regression: the split keeps the existing three families byte-identical ──

test "test_existing_memory_families_unchanged" {
    mr.resetForTest();
    mm.incMemoryCaptured(5);
    mm.incMemoryPushFailure();
    mm.setMemoryHydrationEntries(7);
    var buf: [8192]u8 = undefined;
    const out = try render(&buf);
    try std.testing.expect(contains(out, "# HELP agent_memory_entries_captured_total "));
    try std.testing.expect(contains(out, "# TYPE agent_memory_entries_captured_total counter"));
    try std.testing.expect(contains(out, "agent_memory_entries_captured_total 5"));
    try std.testing.expect(contains(out, "# TYPE agent_memory_push_failures_total counter"));
    try std.testing.expect(contains(out, "agent_memory_push_failures_total 1"));
    try std.testing.expect(contains(out, "# TYPE agent_memory_hydration_window_entries gauge"));
    try std.testing.expect(contains(out, "agent_memory_hydration_window_entries 7"));
}

// ── The six memory-loss families ────────────────────────────────────────────

test "test_metrics_render_memory_loss_families" {
    mr.resetForTest();
    mm.incHydrationDropped(2, 120);
    mm.incCapEvictions(3);
    mm.incCaptureTruncated();
    mm.incCaptureSkipped();
    mm.incSearchZeroHit();
    var buf: [8192]u8 = undefined;
    const out = try render(&buf);
    // Every family renders its HELP line and the exact incremented value.
    try std.testing.expect(contains(out, "# HELP agent_memory_hydration_dropped_entries_total "));
    try std.testing.expect(contains(out, "agent_memory_hydration_dropped_entries_total 2"));
    try std.testing.expect(contains(out, "# HELP agent_memory_hydration_dropped_bytes_total "));
    try std.testing.expect(contains(out, "agent_memory_hydration_dropped_bytes_total 120"));
    try std.testing.expect(contains(out, "# HELP agent_memory_cap_evictions_total "));
    try std.testing.expect(contains(out, "agent_memory_cap_evictions_total 3"));
    try std.testing.expect(contains(out, "# HELP agent_memory_capture_truncated_total "));
    try std.testing.expect(contains(out, "agent_memory_capture_truncated_total 1"));
    try std.testing.expect(contains(out, "# HELP agent_memory_capture_skipped_total "));
    try std.testing.expect(contains(out, "agent_memory_capture_skipped_total 1"));
    try std.testing.expect(contains(out, "# HELP agent_memory_search_zero_hits_total "));
    try std.testing.expect(contains(out, "agent_memory_search_zero_hits_total 1"));
}

test "test_render_no_db_no_alloc" {
    // renderFamilies takes only a writer — no connection or allocator parameter
    // exists (compile-level invariant). A fixed stack buffer proves the render
    // allocates nothing; std.testing.allocator is never handed out.
    mr.resetForTest();
    mm.incCaptureTruncated();
    var buf: [4096]u8 = undefined;
    var w = std.Io.Writer.fixed(&buf);
    try mm.renderFamilies(&w);
    try std.testing.expect(w.buffered().len > 0);
}

// ── Exactness + no-op guards ────────────────────────────────────────────────

test "snapshot reports exact per-counter deltas" {
    const DROPPED_BYTES: usize = 1024;
    mr.resetForTest();
    const before = mm.snapshot();
    mm.incHydrationDropped(4, DROPPED_BYTES);
    mm.incCapEvictions(2);
    mm.incCaptureTruncated();
    mm.incCaptureSkipped();
    mm.incCaptureSkipped();
    mm.incSearchZeroHit();
    const after = mm.snapshot();
    try std.testing.expectEqual(before.hydration_dropped_entries_total + 4, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total + DROPPED_BYTES, after.hydration_dropped_bytes_total);
    try std.testing.expectEqual(before.cap_evictions_total + 2, after.cap_evictions_total);
    try std.testing.expectEqual(before.capture_truncated_total + 1, after.capture_truncated_total);
    try std.testing.expectEqual(before.capture_skipped_total + 2, after.capture_skipped_total);
    try std.testing.expectEqual(before.search_zero_hits_total + 1, after.search_zero_hits_total);
}

test "zero-count increments are no-ops and never activate render" {
    mr.resetForTest();
    mm.incHydrationDropped(0, 999); // no entries dropped → bytes must not move either
    mm.incCapEvictions(0);
    try std.testing.expect(!mm.anyActive());
    const s = mm.snapshot();
    try std.testing.expectEqual(@as(u64, 0), s.hydration_dropped_entries_total);
    try std.testing.expectEqual(@as(u64, 0), s.hydration_dropped_bytes_total);
    try std.testing.expectEqual(@as(u64, 0), s.cap_evictions_total);
}

test "the hydration gauge alone never forces a render" {
    mr.resetForTest();
    mm.setMemoryHydrationEntries(7);
    var buf: [256]u8 = undefined;
    try std.testing.expectEqual(@as(usize, 0), (try render(&buf)).len);
}

test "a loss counter alone activates the exposition (loss is never invisible)" {
    mr.resetForTest();
    mm.incSearchZeroHit();
    var buf: [8192]u8 = undefined;
    try std.testing.expect(contains(try render(&buf), "agent_memory_search_zero_hits_total 1"));
}
