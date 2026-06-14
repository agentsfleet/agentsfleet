//! Global durable-memory telemetry — every `zombie_memory_*` Prometheus family:
//! the capture/hydrate loop counters plus the memory-loss counters (hydration
//! window drops, cap evictions, capture truncations/skips, zero-hit searches).
//! Split out of metrics_runner.zig (which renders these via renderFamilies) to
//! keep both files under the length cap.
//!
//! All families are GLOBAL (unlabelled): per-zombie labels would explode
//! cardinality. The zombie scope rides the structured log line, never a metric
//! label — the inc* functions take counts only, so no identifier can leak in.
//! Lock-free atomic counters, no allocator, no database on the scrape path.
//! Counters are monotonic: only fetchAdd is exposed (resetForTest excepted).
//! Tests live in metrics_memory_test.zig.

const std = @import("std");

const MEM_CAPTURED_NAME = "zombie_memory_entries_captured_total";
const MEM_CAPTURED_HELP = "Durable memory entries persisted via the runner-plane capture push.";
const MEM_PUSH_FAIL_NAME = "zombie_memory_push_failures_total";
const MEM_PUSH_FAIL_HELP = "Memory capture pushes that failed to persist (ERR_MEM_UNAVAILABLE).";
const MEM_HYDRATION_NAME = "zombie_memory_hydration_window_entries";
const MEM_HYDRATION_HELP = "Entry count in the most recent hydration window served to a runner.";
const HYDRATION_DROPPED_ENTRIES_NAME = "zombie_memory_hydration_dropped_entries_total";
const HYDRATION_DROPPED_ENTRIES_HELP = "Durable entries dropped from hydration replies by the byte-budget window (cold tail stays in Postgres).";
const HYDRATION_DROPPED_BYTES_NAME = "zombie_memory_hydration_dropped_bytes_total";
const HYDRATION_DROPPED_BYTES_HELP = "Bytes (key+content+category) dropped from hydration replies by the byte-budget window.";
const CAP_EVICTIONS_NAME = "zombie_memory_cap_evictions_total";
const CAP_EVICTIONS_HELP = "Durable entries deleted by the per-zombie cap eviction after a capture push.";
const CAPTURE_TRUNCATED_NAME = "zombie_memory_capture_truncated_total";
const CAPTURE_TRUNCATED_HELP = "Capture pushes truncated at the push byte budget (tail deltas not persisted).";
const CAPTURE_SKIPPED_NAME = "zombie_memory_capture_skipped_total";
const CAPTURE_SKIPPED_HELP = "Capture deltas skipped by validation (oversized or empty key, content, or category).";
const SEARCH_ZERO_HITS_NAME = "zombie_memory_search_zero_hits_total";
const SEARCH_ZERO_HITS_HELP = "Tenant memory searches that returned zero rows (recall-miss signal).";

// Prometheus exposition format strings — single-sourced (RULE UFS); the format
// arg to writer.print must be comptime, so container-level consts. pub because
// metrics_runner.zig renders its own families with the same formats.
pub const FMT_HELP_TYPE = "# HELP {s} {s}\n# TYPE {s} {s}\n";
pub const FMT_HELP_TYPE_VALUE = "# HELP {s} {s}\n# TYPE {s} {s}\n{s} {d}\n";
pub const TYPE_COUNTER = "counter";
pub const TYPE_GAUGE = "gauge";

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
/// no-op also discards `dropped_bytes` — anyActive() relies on the pair moving
/// together, so never pass (0, nonzero).
pub fn incHydrationDropped(entries: usize, dropped_bytes: usize) void {
    if (entries == 0) return;
    _ = g_hydration_dropped_entries_total.fetchAdd(@intCast(entries), .monotonic); // safe because: independent counter
    _ = g_hydration_dropped_bytes_total.fetchAdd(@intCast(dropped_bytes), .monotonic); // safe because: independent counter
}

/// The per-zombie cap eviction after a capture push deleted `n` rows.
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
// as the existing zombie_runner_* families under concurrent scrapes).
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

/// True once any memory counter has moved — gates rendering so a scrape before
/// any activity stays empty (the gauge and dropped_bytes alone never force a
/// render: dropped_bytes only moves when dropped_entries also moves, so the
/// entries check below subsumes it).
pub fn anyActive() bool {
    const s = snapshot();
    return s.captured_total != 0 or s.push_failures_total != 0 or
        s.hydration_dropped_entries_total != 0 or s.cap_evictions_total != 0 or
        s.capture_truncated_total != 0 or s.capture_skipped_total != 0 or
        s.search_zero_hits_total != 0;
}

// ── Prometheus rendering (called by metrics_runner.renderPrometheus) ────────

/// Render every memory family with HELP/TYPE lines. Reads atomics only — takes
/// no connection or allocator parameter (the scrape path stays database-free).
/// The gauge clamps transient <0.
pub fn renderFamilies(writer: anytype) !void {
    const s = snapshot();
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_CAPTURED_NAME, MEM_CAPTURED_HELP, MEM_CAPTURED_NAME, TYPE_COUNTER, MEM_CAPTURED_NAME, s.captured_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_PUSH_FAIL_NAME, MEM_PUSH_FAIL_HELP, MEM_PUSH_FAIL_NAME, TYPE_COUNTER, MEM_PUSH_FAIL_NAME, s.push_failures_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ MEM_HYDRATION_NAME, MEM_HYDRATION_HELP, MEM_HYDRATION_NAME, TYPE_GAUGE, MEM_HYDRATION_NAME, @max(0, s.hydration_entries) });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ HYDRATION_DROPPED_ENTRIES_NAME, HYDRATION_DROPPED_ENTRIES_HELP, HYDRATION_DROPPED_ENTRIES_NAME, TYPE_COUNTER, HYDRATION_DROPPED_ENTRIES_NAME, s.hydration_dropped_entries_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ HYDRATION_DROPPED_BYTES_NAME, HYDRATION_DROPPED_BYTES_HELP, HYDRATION_DROPPED_BYTES_NAME, TYPE_COUNTER, HYDRATION_DROPPED_BYTES_NAME, s.hydration_dropped_bytes_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ CAP_EVICTIONS_NAME, CAP_EVICTIONS_HELP, CAP_EVICTIONS_NAME, TYPE_COUNTER, CAP_EVICTIONS_NAME, s.cap_evictions_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ CAPTURE_TRUNCATED_NAME, CAPTURE_TRUNCATED_HELP, CAPTURE_TRUNCATED_NAME, TYPE_COUNTER, CAPTURE_TRUNCATED_NAME, s.capture_truncated_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ CAPTURE_SKIPPED_NAME, CAPTURE_SKIPPED_HELP, CAPTURE_SKIPPED_NAME, TYPE_COUNTER, CAPTURE_SKIPPED_NAME, s.capture_skipped_total });
    try writer.print(FMT_HELP_TYPE_VALUE, .{ SEARCH_ZERO_HITS_NAME, SEARCH_ZERO_HITS_HELP, SEARCH_ZERO_HITS_NAME, TYPE_COUNTER, SEARCH_ZERO_HITS_NAME, s.search_zero_hits_total });
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
