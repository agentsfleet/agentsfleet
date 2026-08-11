//! Global durable-memory telemetry — every `agentsfleet_memory_*` family:
//! the capture/hydrate loop counters plus the memory-loss counters (hydration
//! window drops, cap evictions, capture truncations/skips, zero-hit searches).
//! The OTLP registry (`otel_metrics_families.zig`) declares each family by the
//! `*_NAME` constants below; export rides the push pipeline.
//!
//! All families are GLOBAL (unlabelled): per-fleet labels would explode
//! cardinality. The fleet scope rides the structured log line, never a metric
//! label — the inc* functions take counts only, so no identifier can leak in.
//! Lock-free atomic counters, no allocator, no database on the export path.
//! Counters are monotonic: only fetchAdd is exposed (resetForTest excepted).
//! Tests live in metrics_memory_test.zig.

const std = @import("std");

pub const MEM_CAPTURED_NAME = "agentsfleet_memory_entries_captured_total";
pub const MEM_PUSH_FAIL_NAME = "agentsfleet_memory_push_failures_total";
pub const MEM_HYDRATION_NAME = "agentsfleet_memory_hydration_window_entries";
pub const HYDRATION_DROPPED_ENTRIES_NAME = "agentsfleet_memory_hydration_dropped_entries_total";
pub const HYDRATION_DROPPED_BYTES_NAME = "agentsfleet_memory_hydration_dropped_bytes_total";
pub const CAP_EVICTIONS_NAME = "agentsfleet_memory_cap_evictions_total";
pub const CAPTURE_TRUNCATED_NAME = "agentsfleet_memory_capture_truncated_total";
pub const CAPTURE_SKIPPED_NAME = "agentsfleet_memory_capture_skipped_total";
pub const SEARCH_ZERO_HITS_NAME = "agentsfleet_memory_search_zero_hits_total";

var g_captured_total = std.atomic.Value(u64).init(0);
var g_push_failures_total = std.atomic.Value(u64).init(0);
var g_hydration_entries = std.atomic.Value(i64).init(0);
var g_hydration_dropped_entries_total = std.atomic.Value(u64).init(0);
var g_hydration_dropped_bytes_total = std.atomic.Value(u64).init(0);
var g_cap_evictions_total = std.atomic.Value(u64).init(0);
var g_capture_truncated_total = std.atomic.Value(u64).init(0);
var g_capture_skipped_total = std.atomic.Value(u64).init(0);
var g_search_zero_hits_total = std.atomic.Value(u64).init(0);

// ── Push API (called from the memory handlers) ──────────────────────────────

/// `n` memory entries were persisted by a capture push. Global counter (no label).
pub fn incMemoryCaptured(n: usize) void {
    if (n == 0) return;
    _ = g_captured_total.fetchAdd(@intCast(n), .monotonic); // safe because: independent counter
}

/// A memory capture push failed to persist (ERR_MEM_UNAVAILABLE). Global counter.
pub fn incMemoryPushFailure() void {
    _ = g_push_failures_total.fetchAdd(1, .monotonic); // safe because: independent counter
}

/// Record the entry count of the most recent hydration window (gauge, last-writer-wins).
pub fn setMemoryHydrationEntries(n: usize) void {
    g_hydration_entries.store(@intCast(n), .monotonic); // safe because: lone gauge, last-writer-wins
}

/// The category-pinned hydration window dropped `entries` entries totalling
/// `dropped_bytes` (key+content+category) from one hydrate reply. The zero-entries
/// no-op also discards `dropped_bytes` — the two counters move together by
/// design, so never pass (0, nonzero).
pub fn incHydrationDropped(entries: usize, dropped_bytes: usize) void {
    if (entries == 0) return;
    _ = g_hydration_dropped_entries_total.fetchAdd(@intCast(entries), .monotonic); // safe because: independent counter
    _ = g_hydration_dropped_bytes_total.fetchAdd(@intCast(dropped_bytes), .monotonic); // safe because: independent counter
}

/// The per-fleet cap eviction after a capture push deleted `n` rows.
pub fn incCapEvictions(n: u64) void {
    if (n == 0) return;
    _ = g_cap_evictions_total.fetchAdd(n, .monotonic); // safe because: independent counter
}

/// One capture push hit the push byte budget and stopped early (tail not persisted).
pub fn incCaptureTruncated() void {
    _ = g_capture_truncated_total.fetchAdd(1, .monotonic); // safe because: independent counter
}

/// One capture delta was skipped by validation (oversized/empty key, content, or category).
pub fn incCaptureSkipped() void {
    _ = g_capture_skipped_total.fetchAdd(1, .monotonic); // safe because: independent counter
}

/// One tenant memory search returned zero rows (recall-miss signal).
pub fn incSearchZeroHit() void {
    _ = g_search_zero_hits_total.fetchAdd(1, .monotonic); // safe because: independent counter
}

// ── Read API ────────────────────────────────────────────────────────────────

/// Point-in-time copy of every family, for exact-delta test assertions
/// (mirrors metrics.zig's snapshot pattern).
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

// safe because: every load below is .monotonic — these are independent
// monotonic counters with no cross-variable ordering requirement; a snapshot
// is a per-counter point-in-time read, not a consistent cut (same guarantee
// as the existing agentsfleet_runner_* families under concurrent scrapes).
pub fn snapshot() Snapshot {
    return .{
        .captured_total = g_captured_total.load(.monotonic),
        .push_failures_total = g_push_failures_total.load(.monotonic),
        .hydration_entries = g_hydration_entries.load(.monotonic),
        .hydration_dropped_entries_total = g_hydration_dropped_entries_total.load(.monotonic),
        .hydration_dropped_bytes_total = g_hydration_dropped_bytes_total.load(.monotonic),
        .cap_evictions_total = g_cap_evictions_total.load(.monotonic),
        .capture_truncated_total = g_capture_truncated_total.load(.monotonic),
        .capture_skipped_total = g_capture_skipped_total.load(.monotonic),
        .search_zero_hits_total = g_search_zero_hits_total.load(.monotonic),
    };
}

// Test-only reset, consumed by metrics_memory_test.zig (and delegated to by
// metrics_runner.resetForTest so existing call sites reset both modules).
pub fn resetForTest() void {
    g_captured_total.store(0, .release);
    g_push_failures_total.store(0, .release);
    g_hydration_entries.store(0, .release);
    g_hydration_dropped_entries_total.store(0, .release);
    g_hydration_dropped_bytes_total.store(0, .release);
    g_cap_evictions_total.store(0, .release);
    g_capture_truncated_total.store(0, .release);
    g_capture_skipped_total.store(0, .release);
    g_search_zero_hits_total.store(0, .release);
}
