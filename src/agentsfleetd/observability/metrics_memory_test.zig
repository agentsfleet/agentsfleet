//! Black-box tests for metrics_memory — drive the public push API, assert on
//! exact snapshot deltas and the exported OTLP window. The memory families ride
//! the same flush envelope as every other family, so the capture/push/hydration
//! tests keep asserting through that one shared observation surface — pinning
//! that the export cutover changed no exported name, kind, or value.

const std = @import("std");
const mm = @import("metrics_memory.zig");
const mr = @import("metrics_runner.zig");
const window = @import("otel_metrics_window_test.zig");

// ── Pre-split behaviour, now pinned on the wire ─────────────────────────────

test "memory capture counter accumulates and exports" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incMemoryCaptured(3);
    mm.incMemoryCaptured(2);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 5), try window.familyValueWith(body, mm.MEM_CAPTURED_NAME, &.{}));
}

test "memory push-failure counter exports" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incMemoryPushFailure();
    mm.incMemoryPushFailure();
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, mm.MEM_PUSH_FAIL_NAME, &.{}));
}

test "hydration window gauge reflects the last set size" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.setMemoryHydrationEntries(7);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 7), try window.familyValueWith(body, mm.MEM_HYDRATION_NAME, &.{}));
}

test "incMemoryCaptured(0) is a no-op; the family stays live at zero" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incMemoryCaptured(0);
    try std.testing.expectEqual(@as(u64, 0), mm.snapshot().captured_total);
    // The collector deliberately exports zero-valued families every window, so
    // a dashboard series stays live between increments (unlike the retired
    // activity-gated renderer) — the no-op is visible as an exact zero.
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 0), try window.familyValueWith(body, mm.MEM_CAPTURED_NAME, &.{}));
}

// ── Regression: the export keeps the existing three families' identity ──────

test "test_existing_memory_families_unchanged" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incMemoryCaptured(5);
    mm.incMemoryPushFailure();
    mm.setMemoryHydrationEntries(7);
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    // Counters keep their names and export as monotonic cumulative sums.
    const SUM_CUMULATIVE_MONOTONIC = "\"sum\":{\"aggregationTemporality\":2,\"isMonotonic\":true"; // pin test: literal is the contract
    try window.expectFamilyWith(body, "agentsfleet_memory_entries_captured_total", &.{SUM_CUMULATIVE_MONOTONIC}); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 5), try window.familyValueWith(body, mm.MEM_CAPTURED_NAME, &.{}));
    try window.expectFamilyWith(body, "agentsfleet_memory_push_failures_total", &.{SUM_CUMULATIVE_MONOTONIC}); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, mm.MEM_PUSH_FAIL_NAME, &.{}));
    // The hydration level keeps its name and exports in the gauge shape.
    try window.expectFamilyWith(body, "agentsfleet_memory_hydration_window_entries", &.{"\"gauge\":{\"dataPoints\""}); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 7), try window.familyValueWith(body, mm.MEM_HYDRATION_NAME, &.{}));
}

// ── The six memory-loss families ────────────────────────────────────────────

test "the six memory-loss families export with exact incremented values" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incHydrationDropped(2, 120);
    mm.incCapEvictions(3);
    mm.incCaptureTruncated();
    mm.incCaptureSkipped();
    mm.incSearchZeroHit();
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 2), try window.familyValueWith(body, "agentsfleet_memory_hydration_dropped_entries_total", &.{})); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 120), try window.familyValueWith(body, "agentsfleet_memory_hydration_dropped_bytes_total", &.{})); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 3), try window.familyValueWith(body, "agentsfleet_memory_cap_evictions_total", &.{})); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, "agentsfleet_memory_capture_truncated_total", &.{})); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, "agentsfleet_memory_capture_skipped_total", &.{})); // pin test: literal is the contract
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, "agentsfleet_memory_search_zero_hits_total", &.{})); // pin test: literal is the contract
}

test "the memory snapshot path takes no connection and no allocator" {
    // The export path must stay healthy when the datastores are not: the
    // snapshot signature admits no allocator and no connection, which is the
    // compile-level form of the old no-db/no-alloc claim.
    try std.testing.expectEqual(fn () mm.Snapshot, @TypeOf(mm.snapshot));
}

// ── Exactness + no-op guards ────────────────────────────────────────────────

test "snapshot reports exact per-counter deltas" {
    const DROPPED_BYTES: usize = 1024;
    mr.resetForTest();
    defer mr.resetForTest();
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

test "zero-count increments are no-ops" {
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incHydrationDropped(0, 999); // no entries dropped → bytes must not move either
    mm.incCapEvictions(0);
    const s = mm.snapshot();
    try std.testing.expectEqual(@as(u64, 0), s.hydration_dropped_entries_total);
    try std.testing.expectEqual(@as(u64, 0), s.hydration_dropped_bytes_total);
    try std.testing.expectEqual(@as(u64, 0), s.cap_evictions_total);
}

test "a loss counter alone is visible on the wire (loss is never invisible)" {
    const alloc = std.testing.allocator;
    mr.resetForTest();
    defer mr.resetForTest();
    mm.incSearchZeroHit();
    const body = try window.flushWindowJson(alloc);
    defer alloc.free(body);
    try std.testing.expectEqual(@as(i64, 1), try window.familyValueWith(body, mm.SEARCH_ZERO_HITS_NAME, &.{}));
}
