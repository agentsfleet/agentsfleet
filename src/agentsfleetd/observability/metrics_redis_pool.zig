//! Redis pool metrics — Prometheus snapshot of the live request-path Pool.
//!
//! Pool already keeps its own counters (active/idle/dials/poisoned/etc.); this
//! module is a thin singleton registry holding one `*Pool` reference so
//! `metrics_render.zig` can pull `PoolStats` at scrape time without a direct
//! queue-layer dependency. Registration happens once at boot in `serve.zig`;
//! deregistration must fire before the Pool deinits to avoid a dangling read.
//!
//! No per-instance state — there is exactly one request-path Pool per process.
//! Tests register a fake Pool through the same entry point.

const std = @import("std");
const common = @import("common");
const Pool = @import("../queue/redis_pool.zig");

pub const PoolStats = Pool.PoolStats;

var g_registered_pool: ?*Pool = null;
var g_mutex: common.Mutex = .{};

pub fn registerPool(pool: *Pool) void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_registered_pool = pool;
}

pub fn clearRegisteredPool() void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_registered_pool = null;
}

/// Returns a fresh `PoolStats` snapshot from the registered Pool, or null if
/// no Pool has been registered yet (early-boot scrape, or post-teardown).
/// Caller (`metrics_render.zig`) emits no Redis-pool metric lines when null.
///
/// `g_mutex` is held across `pool.stats()` to close a TOCTOU window: without
/// the lock, a concurrent `clearRegisteredPool()` + `api_queue.deinit()` in
/// `serve.zig`'s shutdown path could free the Pool between the pointer-read
/// and the stats() call. Lock ordering is `g_mutex → pool.mutex` (pool's own
/// methods never acquire `g_mutex`), so no deadlock risk.
pub fn snapshot() ?PoolStats {
    g_mutex.lock();
    defer g_mutex.unlock();
    const pool = g_registered_pool orelse return null;
    return pool.stats();
}

test "snapshot returns null when no pool is registered" {
    clearRegisteredPool();
    try std.testing.expect(snapshot() == null);
}

test "clearRegisteredPool is idempotent" {
    clearRegisteredPool();
    clearRegisteredPool();
    try std.testing.expect(snapshot() == null);
}

// Live Pool snapshot rendering is exercised by slice 9's integration test
// (tests/integration/redis_pool_test.zig) — that path connects a real Pool,
// registers it, and asserts the rendered Prometheus text. The unit tests
// above keep the registry semantics honest in isolation.
