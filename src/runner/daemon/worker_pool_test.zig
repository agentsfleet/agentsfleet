//! Unit tests for the worker pool's spawn/join lifecycle (`worker_pool.zig`),
//! independent of any control plane. A worker whose `stop` flag is already set
//! when it starts returns before its first poll — no network, no fork — so these
//! prove the pool's thread-handle allocation, the join-all reap, and per-worker
//! allocator setup/teardown on every supported platform. The concurrency proofs
//! (N workers leasing at once, clean drain under load, concurrent fork/reap) are
//! real-process and live in worker_pool_integration_test.zig (Linux).

const std = @import("std");
const testing = std.testing;
const Config = @import("config.zig");
const worker_pool = @import("worker_pool.zig");
const dts = @import("deadline_test_support.zig");

/// A static daemon Config for the lifecycle tests — `dev_none`, static string
/// fields (no allocator ownership, so no `deinit`). The workers never reach the
/// network (their `stop` flag is pre-set), so the URL is never dialed.
fn staticCfg(worker_count: u32) Config {
    return .{
        .control_plane_url = "http://127.0.0.1:0",
        .runner_token = "agt_rtest",
        .host_id = "pool-test-host",
        .sandbox_tier = .dev_none,
        .workspace_base = "/tmp/agentsfleet-runner-pool-test",
        .network_policy = .deny_all_egress,
        .worker_count = worker_count,
        .cp_deadlines = .{},
        .registry_allowlist = &.{},
        .alloc = testing.allocator,
    };
}

test "pool spawns worker_count threads and joins them all cleanly" {
    const io = @import("common").globalIo();
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var env_map: std.process.Environ.Map = .init(testing.allocator);
    defer env_map.deinit();

    // Pre-set stop so every worker returns before its first poll: the loop
    // condition `!stop and !drain` is false on entry. This exercises spawn → the
    // per-worker allocator scope construct/destroy → join, with no I/O.
    var stop = std.atomic.Value(bool).init(true);
    var drain = std.atomic.Value(bool).init(false);

    const cfg = staticCfg(4);
    var pool = try worker_pool.spawn(io, testing.allocator, try deadlines.start(testing.allocator), cfg, &env_map, &stop, &drain);
    try testing.expectEqual(@as(usize, 4), pool.threads.len); // one handle per worker
    try pool.join(); // must return .ok — a hang is a stuck worker; an error is a leaked worker
}

test "pool drains via the drain flag as well as stop" {
    const io = @import("common").globalIo();
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var env_map: std.process.Environ.Map = .init(testing.allocator);
    defer env_map.deinit();

    // Symmetric to the stop path: a pre-set drain flag also keeps every worker
    // out of its poll loop, so join reaps a quiescent pool.
    var stop = std.atomic.Value(bool).init(false);
    var drain = std.atomic.Value(bool).init(true);

    var pool = try worker_pool.spawn(io, testing.allocator, try deadlines.start(testing.allocator), staticCfg(2), &env_map, &stop, &drain);
    try testing.expectEqual(@as(usize, 2), pool.threads.len);
    try pool.join();
}

test "single-worker pool is the degenerate N=1 case" {
    // Invariant 6: worker_count=1 is one worker thread — today's single-fleet
    // daemon shape. Proves spawn/join holds at the boundary value.
    const io = @import("common").globalIo();
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var env_map: std.process.Environ.Map = .init(testing.allocator);
    defer env_map.deinit();

    var stop = std.atomic.Value(bool).init(true);
    var drain = std.atomic.Value(bool).init(false);

    var pool = try worker_pool.spawn(io, testing.allocator, try deadlines.start(testing.allocator), staticCfg(1), &env_map, &stop, &drain);
    try testing.expectEqual(@as(usize, 1), pool.threads.len);
    try pool.join();
}

test "foldWorkerVerdict: any per-worker leak flag makes the pool verdict .leak" {
    // The pool's teardown verdict is the OR of every worker's own
    // DebugAllocator verdict. Pure fold — no thread, no allocator, no log — so
    // the fail-on-warn runner can assert it. `join` routes this verdict through
    // `leak_guard.check`, which fails the join in Debug builds.
    var flags = [_]std.atomic.Value(bool){
        std.atomic.Value(bool).init(false),
        std.atomic.Value(bool).init(false),
        std.atomic.Value(bool).init(false),
    };
    try testing.expectEqual(std.heap.Check.ok, worker_pool.foldWorkerVerdict(&flags));

    flags[1].store(true, .seq_cst); // one worker reported a leak on teardown
    try testing.expectEqual(std.heap.Check.leak, worker_pool.foldWorkerVerdict(&flags));
}
