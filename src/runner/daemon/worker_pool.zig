//! Fixed-N worker-thread pool for the host runner. The control loop
//! (`loop.runLoop`) owns the host heartbeat and spawns this pool once it is live;
//! each worker thread then runs the existing `loop.pollAndProcess` (lease →
//! execute → report) verbatim, lifting per-host throughput from one concurrent
//! fleet to `cfg.worker_count`. No control-plane change is needed for
//! correctness: the per-fleet `affinity.claim` admits exactly one of N racing
//! pollers, so two workers never run the same fleet.
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
const call_deadline = @import("call_deadline");
const client_mod = @import("control_plane_client.zig");
const loop = @import("loop.zig");

const leak_guard = logging.leak_guard;

const log = logging.scoped(.fleet_runner);

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
    /// The ONE process scheduler, borrowed from the runner root. Every worker
    /// arms against it; no worker ever creates its own. The root joins the pool
    /// before deiniting it, so it outlives every worker that can arm.
    sched: *call_deadline.ProcessScheduler,
    env_map: *const std.process.Environ.Map,
    stop: *std.atomic.Value(bool),
    drain: *std.atomic.Value(bool),
    /// The worker stores its own `DebugAllocator.deinit()` verdict here at
    /// teardown (`true` == `.leak`); the pool folds every slot at `join`. Points
    /// into `Pool.leak_flags`, which outlives the worker (joined before freed).
    leak_slot: *std.atomic.Value(bool),
};

/// A running fixed-N pool. `join()` blocks until every worker has returned and
/// frees the thread handles. Construct via `spawn`.
pub const Pool = struct {
    alloc: std.mem.Allocator,
    threads: []std.Thread,
    /// One leak flag per worker; a worker stores `deinit() == .leak` into its
    /// slot at teardown and `join` folds them into the pool's verdict.
    leak_flags: []std.atomic.Value(bool),

    /// Block until every worker thread returns, free the handle + flag slices,
    /// then surface the folded per-worker leak verdict (`leak_guard.check`):
    /// any worker that leaked fails `join` in Debug builds and logs in release.
    /// The caller must have already set `stop`/`drain` (the control loop does
    /// this on its exit path) or the workers would never leave their poll loop.
    pub fn join(self: Pool) leak_guard.LeakError!void {
        for (self.threads) |t| t.join();
        log.debug("worker_pool_joined", .{ .workers = self.threads.len });
        const verdict = foldWorkerVerdict(self.leak_flags);
        self.alloc.free(self.threads);
        self.alloc.free(self.leak_flags);
        return leak_guard.check(verdict, "worker");
    }
};

/// Fold the per-worker leak flags into one teardown verdict — any worker that
/// reported a leak makes the pool's verdict `.leak`. Pure (no logging) so the
/// runner's fail-on-warn test harness can assert it without emitting a log.
pub fn foldWorkerVerdict(leak_flags: []const std.atomic.Value(bool)) std.heap.Check {
    for (leak_flags) |*f| {
        if (f.load(.seq_cst)) return .leak;
    }
    return .ok;
}

/// Spawn `cfg.worker_count` worker threads, each running `workerLoop` with its
/// own allocator scope + client. Returns a `Pool` the caller joins on shutdown.
/// On a partial-spawn failure, the workers already up are told to stop and joined
/// (so no thread leaks) before the error is returned.
pub fn spawn(
    io: std.Io,
    alloc: std.mem.Allocator,
    sched: *call_deadline.ProcessScheduler,
    cfg: Config,
    env_map: *const std.process.Environ.Map,
    stop: *std.atomic.Value(bool),
    drain: *std.atomic.Value(bool),
) PoolError!Pool {
    const threads = try alloc.alloc(std.Thread, cfg.worker_count);
    errdefer alloc.free(threads);
    const leak_flags = try alloc.alloc(std.atomic.Value(bool), cfg.worker_count);
    errdefer alloc.free(leak_flags);
    for (leak_flags) |*f| f.* = std.atomic.Value(bool).init(false);
    var spawned: usize = 0;
    errdefer {
        // Partial spawn: unblock the workers already up and join them (they
        // write their leak slot on the way out). The threads/leak_flags slices
        // are freed by the two errdefers above once this join returns.
        stop.store(true, .seq_cst);
        for (threads[0..spawned]) |t| t.join();
    }
    while (spawned < cfg.worker_count) : (spawned += 1) {
        const ctx = WorkerContext{
            .io = io,
            .index = @intCast(spawned),
            .cfg = cfg,
            .sched = sched,
            .env_map = env_map,
            .stop = stop,
            .drain = drain,
            .leak_slot = &leak_flags[spawned],
        };
        threads[spawned] = try std.Thread.spawn(.{}, workerLoop, .{ctx});
    }
    log.debug("worker_pool_spawned", .{ .workers = cfg.worker_count });
    return .{ .alloc = alloc, .threads = threads, .leak_flags = leak_flags };
}

/// One worker: lease → execute → report (the existing `pollAndProcess`, verbatim)
/// until `stop`/`drain` is set, each with its OWN allocator scope and client. The
/// allocator is per-thread so workers never contend on a shared allocator mutex;
/// the client is per-worker state (persistent keep-alive connection + per-call
/// deadlines), never shared across threads. The SCHEDULER is the one thing all
/// workers share — one worker thread bounds every call in the process.
fn workerLoop(ctx: WorkerContext) void {
    var gpa: std.heap.DebugAllocator(.{}) = .{};
    // Record this worker's leak verdict for the pool to fold at join. Registered
    // first → runs LAST (LIFO), after `cp.deinit()` has freed its allocations,
    // so the verdict reflects a fully-drained worker.
    defer ctx.leak_slot.store(gpa.deinit() == .leak, .seq_cst);
    const alloc = gpa.allocator();

    var cp = client_mod.init(alloc, ctx.io, ctx.sched, ctx.cfg.control_plane_url);
    defer cp.deinit();
    log.debug("worker_started", .{ .index = ctx.index });
    while (!ctx.stop.load(.seq_cst) and !ctx.drain.load(.seq_cst)) {
        loop.pollAndProcess(ctx.io, alloc, &cp, ctx.cfg.runner_token, ctx.cfg, ctx.env_map);
    }
    log.debug("worker_stopped", .{ .index = ctx.index });
}

// Tests live in worker_pool_test.zig (unit: spawn/join lifecycle) and
// worker_pool_integration_test.zig (Linux: N concurrent leases, no double-claim,
// clean drain) — kept out of this file to hold the line budget.
