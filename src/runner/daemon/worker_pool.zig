//! Fixed-N worker-thread pool for the host runner. The control loop
//! (`loop.runLoop`) owns the host heartbeat and spawns this pool once it is live;
//! each worker thread then runs the existing `loop.pollAndProcess` (lease →
//! execute → report) verbatim, lifting per-host throughput from one concurrent
//! agent to `cfg.worker_count`. No control-plane change is needed for
//! correctness: the per-zombie `affinity.claim` admits exactly one of N racing
//! pollers, so two workers never run the same zombie.
//!
//! Each worker owns an INDEPENDENT allocator scope (its own `DebugAllocator`) and
//! a fresh control-plane client, so there is no cross-worker allocator mutex to
//! serialise on and no shared mutable state between workers. Children are still
//! forked only via `std.process.spawn` (async-signal-safe post-fork), so forking
//! from this multithreaded daemon is safe by construction.
//!
//! Shutdown is cooperative: the control loop sets `stop`/`drain`; each worker
//! checks them at its between-lease boundary, finishes any in-flight child, takes
//! no new lease, and returns. `join()` then reaps every worker thread. A partial
//! spawn failure stops and joins the workers already up before surfacing the error.

const std = @import("std");
const logging = @import("log");

const Config = @import("config.zig");
const client_mod = @import("control_plane_client.zig");
const loop = @import("loop.zig");

const log = logging.scoped(.zombie_runner);

/// Spawn failure: either the threads handle could not be allocated, or the OS
/// refused a thread. The caller (control loop) logs and exits; workers already
/// spawned are joined before the error propagates.
pub const PoolError = std.mem.Allocator.Error || std.Thread.SpawnError;

/// Per-worker context, copied by value into each spawned thread. The pointers
/// (`stop`/`drain`/`env_map`) and `cfg`'s slices outlive the pool: the control
/// loop joins every worker before its frame (and `cfg`) is torn down.
const WorkerContext = struct {
    io: std.Io,
    index: u32,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    stop: *std.atomic.Value(bool),
    drain: *std.atomic.Value(bool),
};

/// A running fixed-N pool. `join()` blocks until every worker has returned and
/// frees the thread handles. Construct via `spawn`.
pub const Pool = struct {
    alloc: std.mem.Allocator,
    threads: []std.Thread,

    /// Block until every worker thread returns, then free the handle slice. The
    /// caller must have already set `stop`/`drain` (the control loop does this on
    /// its exit path) or the workers would never leave their poll loop.
    pub fn join(self: Pool) void {
        for (self.threads) |t| t.join();
        log.info("worker_pool_joined", .{ .workers = self.threads.len });
        self.alloc.free(self.threads);
    }
};

/// Spawn `cfg.worker_count` worker threads, each running `workerLoop` with its
/// own allocator scope + client. Returns a `Pool` the caller joins on shutdown.
/// On a partial-spawn failure, the workers already up are told to stop and joined
/// (so no thread leaks) before the error is returned.
pub fn spawn(
    io: std.Io,
    alloc: std.mem.Allocator,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    stop: *std.atomic.Value(bool),
    drain: *std.atomic.Value(bool),
) PoolError!Pool {
    const threads = try alloc.alloc(std.Thread, cfg.worker_count);
    var spawned: usize = 0;
    errdefer {
        // Partial spawn: unblock the workers already up, join them, free.
        stop.store(true, .seq_cst);
        for (threads[0..spawned]) |t| t.join();
        alloc.free(threads);
    }
    while (spawned < cfg.worker_count) : (spawned += 1) {
        const ctx = WorkerContext{
            .io = io,
            .index = @intCast(spawned),
            .cfg = cfg,
            .env_map = env_map,
            .stop = stop,
            .drain = drain,
        };
        threads[spawned] = try std.Thread.spawn(.{}, workerLoop, .{ctx});
    }
    log.info("worker_pool_spawned", .{ .workers = cfg.worker_count });
    return .{ .alloc = alloc, .threads = threads };
}

/// One worker: lease → execute → report (the existing `pollAndProcess`, verbatim)
/// until `stop`/`drain` is set, each with its OWN allocator scope and client. The
/// allocator is per-thread so workers never contend on a shared allocator mutex;
/// the client is per-worker state (persistent keep-alive connection + per-call
/// socket deadlines), never shared across threads.
fn workerLoop(ctx: WorkerContext) void {
    var gpa: std.heap.DebugAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var cp = client_mod.init(alloc, ctx.io, ctx.cfg.control_plane_url);
    defer cp.deinit();
    log.info("worker_started", .{ .index = ctx.index });
    while (!ctx.stop.load(.seq_cst) and !ctx.drain.load(.seq_cst)) {
        loop.pollAndProcess(ctx.io, alloc, &cp, ctx.cfg.runner_token, ctx.cfg, ctx.env_map);
    }
    log.info("worker_stopped", .{ .index = ctx.index });
}

// Tests live in worker_pool_test.zig (unit: spawn/join lifecycle) and
// worker_pool_integration_test.zig (Linux: N concurrent leases, no double-claim,
// clean drain) — kept out of this file to hold the line budget.
