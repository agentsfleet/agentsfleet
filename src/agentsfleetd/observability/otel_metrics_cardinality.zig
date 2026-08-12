//! Attribution budget for the one unbounded metric dimension that survives:
//! the exact `gen_ai.request.model` value.
//!
//! Workspace identity used to be tracked here. It no longer reaches an OTLP
//! metric at all — exact per-workspace cost is a Postgres query against the
//! execution-telemetry rows, which is durable and already the money truth,
//! whereas a metric label accumulates series across replicas and restarts that
//! no process-local guard can bound.
//!
//! What remains is a budget over distinct (provider, model) pairs, sized so the
//! flush window provably stays under the aggregator's distinct-series ceiling:
//! the first `ATTRIBUTION_CAP` pairs keep exact model attribution, every later
//! pair still exports its measurement but without the model attribute. The cap
//! is derived from the registry's fixed attribute sets, never hand-picked.
//!
//! A small mutex-guarded hash set — this runs post-commit off the money path,
//! never on a request hot path, so a lock here is fine (and simpler-correct
//! than the lock-free ring it feeds).

const std = @import("std");
const common = @import("common");
const semconv = @import("semconv.zig");
const families = @import("otel_metrics_families.zig");

/// Distinct (provider, model) pairs that may carry exact model attribution.
/// Derived from the COST sub-budget, not the whole aggregator ceiling: the
/// runtime families occupy their own declared share of the ceiling, so adding
/// one can never shrink this cap (the arithmetic the derived-ceiling tests pin).
pub const ATTRIBUTION_CAP: usize = semconv.modelAttributionCap(families.COST_SERIES_BUDGET);

comptime {
    // A zero cap would silently drop model attribution everywhere, which is the
    // behaviour the registry's series arithmetic exists to prevent.
    std.debug.assert(ATTRIBUTION_CAP > 0);
}

// Second Wyhash seed (golden ratio) for a 128-bit composite digest. Two
// independent 64-bit hashes must BOTH collide for a false match, dropping the
// collision probability from ~2.7e-16 to ~7e-32 — effectively zero. We store
// the digest, not the bytes, so the guard stays fixed-size and allocation-free.
const PAIR_HASH_SEED_B: u64 = 0x9e3779b97f4a7c15;
/// Separator between the two hashed fields so ("ab","c") and ("a","bc") differ.
const PAIR_SEPARATOR = [_]u8{0};

var g_mutex: common.Mutex = .{};
var g_hashes: [ATTRIBUTION_CAP][2]u64 = undefined;
var g_count: usize = 0;

fn hashPair(seed: u64, provider: []const u8, model: []const u8) u64 {
    var hasher = std.hash.Wyhash.init(seed);
    hasher.update(provider);
    hasher.update(&PAIR_SEPARATOR);
    hasher.update(model);
    return hasher.final();
}

fn digest(provider: []const u8, model: []const u8) [2]u64 {
    return .{ hashPair(0, provider, model), hashPair(PAIR_HASH_SEED_B, provider, model) };
}

/// True if this (provider, model) pair may carry exact `gen_ai.request.model`
/// attribution. Tracks distinct pairs up to the derived cap; once the cap is
/// reached, only already-admitted pairs stay attributed and new ones lose the
/// attribute. The sample itself is always still exported — the caller counts
/// the omission so the gap is visible on the dashboard.
pub fn admitModel(provider: []const u8, model: []const u8) bool {
    const h = digest(provider, model);
    g_mutex.lock();
    defer g_mutex.unlock();
    var i: usize = 0;
    while (i < g_count) : (i += 1) {
        if (g_hashes[i][0] == h[0] and g_hashes[i][1] == h[1]) return true;
    }
    if (g_count >= ATTRIBUTION_CAP) return false;
    g_hashes[g_count] = h;
    g_count += 1;
    return true;
}

/// Number of distinct pairs currently attributed (for tests / diagnostics).
pub fn trackedCount() usize {
    g_mutex.lock();
    defer g_mutex.unlock();
    return g_count;
}

/// Clear the tracked set, opening a fresh attribution window. Called by the
/// metrics flush once it has drained the window this set governed, so the
/// budget tracks the models active in a window rather than the first ones the
/// process ever saw.
pub fn reset() void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_count = 0;
}
