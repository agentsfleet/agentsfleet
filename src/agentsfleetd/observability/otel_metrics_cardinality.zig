//! Cardinality guard for the high-cardinality `workspace` metric label.
//! Bounds the number of distinct workspace values that ever leave the process
//! as a label: the first WORKSPACE_CARDINALITY_CAP distinct workspaces are
//! retained, every later one has its `workspace` label dropped (the sample is
//! still emitted, just without the unbounded dimension).
//!
//! A small mutex-guarded hash set — this runs post-commit off the money path,
//! never on a request hot path, so a lock here is fine (and simpler-correct
//! than the lock-free ring it feeds).

const std = @import("std");
const common = @import("common");

pub const WORKSPACE_CARDINALITY_CAP: usize = 100;

var g_mutex: common.Mutex = .{};
var g_hashes: [WORKSPACE_CARDINALITY_CAP]u64 = undefined;
var g_count: usize = 0;

/// True if `workspace` should be retained as a label. Tracks distinct values
/// up to the cap; once the cap is reached, only already-seen workspaces stay
/// labelled and new ones are dropped.
pub fn allowWorkspace(workspace: []const u8) bool {
    const h = std.hash.Wyhash.hash(0, workspace);
    g_mutex.lock();
    defer g_mutex.unlock();
    var i: usize = 0;
    while (i < g_count) : (i += 1) {
        if (g_hashes[i] == h) return true;
    }
    if (g_count >= WORKSPACE_CARDINALITY_CAP) return false;
    g_hashes[g_count] = h;
    g_count += 1;
    return true;
}

/// Number of distinct workspaces currently tracked (for tests / diagnostics).
pub fn trackedCount() usize {
    g_mutex.lock();
    defer g_mutex.unlock();
    return g_count;
}

/// Clear the tracked set. Test-only; the guard is process-lifetime in prod.
pub fn reset() void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_count = 0;
}
