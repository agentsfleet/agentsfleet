//! Global durable-memory telemetry — every `agentsfleet_memory_*` family:
//! the capture/hydrate loop counters plus the memory-loss counters (hydration
//! window drops, cap evictions, capture truncations/skips, zero-hit searches).
//! The OTLP registry (`otel_metrics_families.zig`) declares each family by the
//! `*_NAME` constants below; storage lives in the generated instrument layer
//! (otel_instruments.zig) and export rides the push pipeline.
//!
//! All families are GLOBAL (unlabelled): per-fleet labels would explode
//! cardinality. The fleet scope rides the structured log line, never a metric
//! label — the inc* functions take counts only, so no identifier can leak in.
//! Counters are monotonic: only additive writers are exposed (resetForTest
//! excepted). Tests live in metrics_memory_test.zig.

const std = @import("std");
const instruments = @import("otel_instruments.zig");

pub const MEM_CAPTURED_NAME = "agentsfleet_memory_entries_captured_total";
pub const MEM_PUSH_FAIL_NAME = "agentsfleet_memory_push_failures_total";
pub const MEM_HYDRATION_NAME = "agentsfleet_memory_hydration_window_entries";
pub const HYDRATION_DROPPED_ENTRIES_NAME = "agentsfleet_memory_hydration_dropped_entries_total";
pub const HYDRATION_DROPPED_BYTES_NAME = "agentsfleet_memory_hydration_dropped_bytes_total";
pub const CAP_EVICTIONS_NAME = "agentsfleet_memory_cap_evictions_total";
pub const CAPTURE_TRUNCATED_NAME = "agentsfleet_memory_capture_truncated_total";
pub const CAPTURE_SKIPPED_NAME = "agentsfleet_memory_capture_skipped_total";
pub const SEARCH_ZERO_HITS_NAME = "agentsfleet_memory_search_zero_hits_total";

// ── Push API (called from the memory handlers) ──────────────────────────────

/// `n` memory entries were persisted by a capture push. Global counter (no label).
pub fn incMemoryCaptured(n: usize) void {
    if (n == 0) return;
    instruments.add(.memory_entries_captured, .{}, @intCast(n));
}

/// A memory capture push failed to persist (ERR_MEM_UNAVAILABLE). Global counter.
pub fn incMemoryPushFailure() void {
    instruments.inc(.memory_push_failures, .{});
}

/// Record the entry count of the most recent hydration window (gauge, last-writer-wins).
pub fn setMemoryHydrationEntries(n: usize) void {
    instruments.set(.memory_hydration_window_entries, .{}, @intCast(n));
}

/// The category-pinned hydration window dropped `entries` entries totalling
/// `dropped_bytes` (key+content+category) from one hydrate reply. The zero-entries
/// no-op also discards `dropped_bytes` — the two counters move together by
/// design, so never pass (0, nonzero).
pub fn incHydrationDropped(entries: usize, dropped_bytes: usize) void {
    if (entries == 0) return;
    instruments.add(.memory_hydration_dropped_entries, .{}, @intCast(entries));
    instruments.add(.memory_hydration_dropped_bytes, .{}, @intCast(dropped_bytes));
}

/// The per-fleet cap eviction after a capture push deleted `n` rows.
pub fn incCapEvictions(n: u64) void {
    if (n == 0) return;
    instruments.add(.memory_cap_evictions, .{}, n);
}

/// One capture push hit the push byte budget and stopped early (tail not persisted).
pub fn incCaptureTruncated() void {
    instruments.inc(.memory_capture_truncated, .{});
}

/// One capture delta was skipped by validation (oversized/empty key, content, or category).
pub fn incCaptureSkipped() void {
    instruments.inc(.memory_capture_skipped, .{});
}

/// One tenant memory search returned zero rows (recall-miss signal).
pub fn incSearchZeroHit() void {
    instruments.inc(.memory_search_zero_hits, .{});
}

// ── Read API ────────────────────────────────────────────────────────────────

/// Point-in-time copy of every family, for exact-delta test assertions
/// (mirrors metrics_counters.zig's snapshot pattern).
pub const Snapshot = struct {
    captured_total: u64,
    push_failures_total: u64,
    hydration_entries: i64,
    hydration_dropped_entries_total: u64,
    hydration_dropped_bytes_total: u64,
    cap_evictions_total: u64,
    capture_truncated_total: u64,
    capture_skipped_total: u64,
    search_zero_hits_total: u64,
};

comptime {
    std.debug.assert(@sizeOf(Snapshot) == 9 * @sizeOf(u64));
}

pub fn snapshot() Snapshot {
    return .{
        .captured_total = instruments.snapshotCell(.memory_entries_captured, .{}),
        .push_failures_total = instruments.snapshotCell(.memory_push_failures, .{}),
        // The setter takes a usize entry count, so the cell can never exceed
        // i64; the clamp keeps the cast total rather than trusting that.
        .hydration_entries = @intCast(@min(instruments.snapshotCell(.memory_hydration_window_entries, .{}), std.math.maxInt(i64))),
        .hydration_dropped_entries_total = instruments.snapshotCell(.memory_hydration_dropped_entries, .{}),
        .hydration_dropped_bytes_total = instruments.snapshotCell(.memory_hydration_dropped_bytes, .{}),
        .cap_evictions_total = instruments.snapshotCell(.memory_cap_evictions, .{}),
        .capture_truncated_total = instruments.snapshotCell(.memory_capture_truncated, .{}),
        .capture_skipped_total = instruments.snapshotCell(.memory_capture_skipped, .{}),
        .search_zero_hits_total = instruments.snapshotCell(.memory_search_zero_hits, .{}),
    };
}

// Test-only reset, consumed by metrics_memory_test.zig (and delegated to by
// metrics_runner.resetForTest so existing call sites reset both modules).
pub fn resetForTest() void {
    instruments.resetCellsForTest(&.{
        .memory_entries_captured,
        .memory_push_failures,
        .memory_hydration_window_entries,
        .memory_hydration_dropped_entries,
        .memory_hydration_dropped_bytes,
        .memory_cap_evictions,
        .memory_capture_truncated,
        .memory_capture_skipped,
        .memory_search_zero_hits,
    });
}
