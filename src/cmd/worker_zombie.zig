//! Zombie worker — claims and runs a single Zombie event loop.
//!
//! One worker thread per Zombie. The thread connects its own Redis client,
//! claims the Zombie from Postgres, enters the event loop, and runs until
//! shutdown OR a per-zombie cancel flag flips OR the Zombie is killed.
//!
//! Crash recovery: on restart, claimZombie loads the last Postgres checkpoint.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const queue_redis = @import("../queue/redis_client.zig");
const balance_policy = @import("../config/balance_policy.zig");
const event_loop = @import("../zombie/event_loop.zig");
const executor_client = @import("../executor/client.zig");
const zombie_config = @import("../zombie/config.zig");
const error_codes = @import("../errors/error_registry.zig");
const logging = @import("log");
const telemetry_mod = @import("../observability/telemetry.zig");
const worker_state_mod = @import("worker/state.zig");

const log = logging.scoped(.zombie_worker);

pub const ZombieWorkerConfig = struct {
    pool: *pg.Pool,
    zombie_id: []const u8,
    /// shutdown_requested flag — true means stop (inverted before passing to event loop).
    shutdown_requested: *const std.atomic.Value(bool),
    executor: ?*executor_client.ExecutorClient,
    workspace_path: []const u8 = "/tmp/zombie",
    telemetry: ?*telemetry_mod.Telemetry = null,
    /// Per-zombie cancel flag, owned by the watcher's ZombieRuntime.
    /// Watcher flips it on `zombie_status_changed status=killed|paused`;
    /// the per-zombie thread observes it via watchShutdown and exits.
    cancel_flag: *std.atomic.Value(bool),
    /// §9 hot-reload signal, owned by the watcher's ZombieRuntime.
    /// Watcher flips it on `zombie_config_changed`; the per-zombie
    /// thread reads + clears it between events and reloads
    /// `session.config` from PG.
    reload_pending: *std.atomic.Value(bool),
    /// Process-wide drain state. SIGTERM-driven graceful shutdown
    /// flips this; per-zombie watchShutdown observes `!isAcceptingWork()`
    /// and flips `running` so the event loop exits.
    worker_state: *const worker_state_mod.WorkerState,
    /// Resolved once in worker.zig from `BALANCE_EXHAUSTED_POLICY`.
    balance_policy: balance_policy.Policy,
};

/// Entry point for a Zombie worker thread.
/// Connects to Redis, claims the Zombie, and enters the event loop.
/// Returns when the running flag is set to false (shutdown) or on fatal error.
pub fn zombieWorkerLoop(alloc: std.mem.Allocator, cfg: ZombieWorkerConfig) void {
    log.info("zombie_worker.start", .{ .zombie_id = cfg.zombie_id });

    var redis = connectRedis(alloc) orelse return;
    defer redis.deinit();
    // Dedicated PUBLISH-only client for the activity channel. Decoupling
    // pub/sub from stream commands prevents per-frame PUBLISH from
    // contending on the queue client's mutex during chunk bursts.
    var redis_publish = connectRedis(alloc) orelse return;
    defer redis_publish.deinit();

    var session = claimOrReturn(alloc, cfg) orelse return;
    defer session.deinit(alloc);

    const exec_ref = cfg.executor orelse {
        log.err("zombie_worker.no_executor", .{ .zombie_id = cfg.zombie_id, .error_code = error_codes.ERR_EXEC_STARTUP_POSTURE });
        return;
    };

    // shutdown_requested=true means stop; event loop expects running=true to continue.
    var running = std.atomic.Value(bool).init(true);
    const watcher = std.Thread.spawn(.{}, watchShutdown, .{ cfg.shutdown_requested, cfg.cancel_flag, cfg.worker_state, &running }) catch {
        log.err("zombie_worker.watcher_spawn_failed", .{ .zombie_id = cfg.zombie_id });
        return;
    };
    defer {
        running.store(false, .release);
        watcher.join();
    }

    event_loop.runEventLoop(alloc, &session, .{
        .pool = cfg.pool,
        .redis = &redis,
        .redis_publish = &redis_publish,
        .executor = exec_ref,
        .running = &running,
        .workspace_path = cfg.workspace_path,
        .telemetry = cfg.telemetry,
        .reload_pending = cfg.reload_pending,
        .balance_policy = cfg.balance_policy,
    });
    log.info("zombie_worker.stopped", .{ .zombie_id = cfg.zombie_id });
}

fn claimOrReturn(alloc: std.mem.Allocator, cfg: ZombieWorkerConfig) ?event_loop.ZombieSession {
    const session = event_loop.claimZombie(alloc, cfg.zombie_id, cfg.pool) catch |err| {
        log.err("zombie_worker.claim_failed", .{ .zombie_id = cfg.zombie_id, .error_code = error_codes.ERR_ZOMBIE_CLAIM_FAILED, .err = @errorName(err) });
        return null;
    };
    log.info("zombie_worker.claimed", .{ .zombie_id = cfg.zombie_id, .name = session.config.name });
    return session;
}

/// Query core.zombies for active zombies and return their IDs.
/// Called at worker startup to discover which Zombies to claim, plus
/// each `reconcileSpawnActive` tick to heal install-time orphans.
pub fn listActiveZombieIds(pool: *pg.Pool, alloc: std.mem.Allocator) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text FROM core.zombies
        \\WHERE status = $1
        \\ORDER BY created_at ASC
    , .{zombie_config.ZombieStatus.active.toSlice()}));
    defer q.deinit();

    var ids: std.ArrayList([]const u8) = .{};
    errdefer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        try ids.append(alloc, id);
    }
    return ids.toOwnedSlice(alloc);
}

/// How far back the reconcile sweep looks when scanning for state drift
/// on non-active rows. The reconcile cadence is ≈30s (`reconcile_every_ticks
/// × block_ms`); a 5-minute window covers 10× the cadence and any realistic
/// retry / network blip without growing unbounded as historical killed
/// zombies accumulate (greptile P? on PR #251 — original query was
/// `WHERE status != 'active'` with no time bound, which scaled with the
/// total killed-row count fleet-wide).
const reconcile_recent_window_ms: i64 = 5 * 60 * 1000;

/// Query core.zombies for zombies whose status went non-active recently —
/// killed or paused, with `updated_at` inside the reconcile window.
/// Used by the watcher's reconcile sweep to catch state drift where a
/// `publishKillSignal` (or pause signal) failed to XADD: PG row says
/// non-active, worker thread still running. Reconcile cancels each such
/// id idempotently.
///
/// Time-bounded so the result set stays small as the killed-row count
/// grows monotonically across the table's lifetime. A worker that missed
/// a kill more than `reconcile_recent_window_ms` ago is a different
/// failure mode than this branch is designed to heal — operator restart
/// is the recovery path for that.
pub fn listNonActiveZombieIds(pool: *pg.Pool, alloc: std.mem.Allocator) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    const cutoff_ms = std.time.milliTimestamp() - reconcile_recent_window_ms;
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text FROM core.zombies
        \\WHERE status != $1 AND updated_at >= $2
        \\ORDER BY updated_at DESC
    , .{ zombie_config.ZombieStatus.active.toSlice(), cutoff_ms }));
    defer q.deinit();

    var ids: std.ArrayList([]const u8) = .{};
    errdefer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        try ids.append(alloc, id);
    }
    return ids.toOwnedSlice(alloc);
}


/// Polls the global shutdown flag, the per-zombie cancel flag, and the
/// WorkerState drain phase, flipping `running` to false when any of them
/// fires. Per-zombie cancel propagates a PATCH .../zombies/{id} with
/// body {status:"killed"}; drain phase propagates SIGTERM-driven graceful
/// shutdown.
fn watchShutdown(
    shutdown: *const std.atomic.Value(bool),
    cancel: *std.atomic.Value(bool),
    drain: *const worker_state_mod.WorkerState,
    running: *std.atomic.Value(bool),
) void {
    // Synchronization contract: each flag is written once with .release by the
    // signal handler / watcher / event loop teardown, and observed here with
    // .acquire. This is the canonical "publish-once, consume-many" pattern —
    // .acq_rel is unnecessary since this thread never writes the input flags,
    // only reads them.
    while (running.load(.acquire)) {
        if (shutdown.load(.acquire)) break;
        if (cancel.load(.acquire)) break;
        if (!drain.isAcceptingWork()) break;
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    // safe because: paired with .acquire load on the spawning thread before .join()
    running.store(false, .release);
}

fn connectRedis(alloc: std.mem.Allocator) ?queue_redis.Client {
    return queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        log.err("zombie_worker.redis_unavailable", .{
            .error_code = error_codes.ERR_STARTUP_REDIS_CONNECT,
            .err = @errorName(err),
        });
        return null;
    };
}

test "ZombieWorkerConfig has required fields" {
    // Compile-time check that the struct has the expected shape.
    const cfg: ZombieWorkerConfig = undefined;
    _ = cfg.pool;
    _ = cfg.zombie_id;
    _ = cfg.shutdown_requested;
    _ = cfg.executor;
}
