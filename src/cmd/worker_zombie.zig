// M2_001: Zombie worker — claims and runs a single Zombie event loop.
//
// One worker thread per Zombie (1:1 mapping for v0.6.0).
// The thread connects its own Redis client, claims the Zombie from Postgres,
// enters the event loop, and runs until shutdown or the Zombie is killed.
//
// Crash recovery: on restart, claimZombie loads the last Postgres checkpoint.

const std = @import("std");
const pg = @import("pg");
const queue_redis = @import("../queue/redis_client.zig");
const event_loop = @import("../zombie/event_loop.zig");
const executor_client = @import("../executor/client.zig");
const error_codes = @import("../errors/codes.zig");
const obs_log = @import("../observability/logging.zig");

const log = std.log.scoped(.zombie_worker);

pub const ZombieWorkerConfig = struct {
    pool: *pg.Pool,
    zombie_id: []const u8,
    /// shutdown_requested flag — true means stop (inverted before passing to event loop).
    shutdown_requested: *const std.atomic.Value(bool),
    executor: ?*executor_client.ExecutorClient,
    workspace_path: []const u8 = "/tmp/zombie",
};

/// Entry point for a Zombie worker thread.
/// Connects to Redis, claims the Zombie, and enters the event loop.
/// Returns when the running flag is set to false (shutdown) or on fatal error.
pub fn zombieWorkerLoop(alloc: std.mem.Allocator, cfg: ZombieWorkerConfig) void {
    log.info("zombie_worker.start zombie_id={s}", .{cfg.zombie_id});

    var redis = connectRedis(alloc) orelse return;
    defer redis.deinit();

    var session = claimOrReturn(alloc, cfg) orelse return;
    defer session.deinit(alloc);

    const exec_ref = cfg.executor orelse {
        log.err("zombie_worker.no_executor zombie_id={s} error_code={s}", .{ cfg.zombie_id, error_codes.ERR_EXEC_STARTUP_POSTURE });
        return;
    };

    // shutdown_requested=true means stop; event loop expects running=true to continue.
    var running = std.atomic.Value(bool).init(true);
    const watcher = std.Thread.spawn(.{}, watchShutdown, .{ cfg.shutdown_requested, &running }) catch {
        log.err("zombie_worker.watcher_spawn_fail zombie_id={s}", .{cfg.zombie_id});
        return;
    };
    defer { running.store(false, .release); watcher.join(); }

    event_loop.runEventLoop(alloc, &session, .{
        .pool = cfg.pool,
        .redis = &redis,
        .executor = exec_ref,
        .running = &running,
        .workspace_path = cfg.workspace_path,
    });
    log.info("zombie_worker.stopped zombie_id={s}", .{cfg.zombie_id});
}

fn claimOrReturn(alloc: std.mem.Allocator, cfg: ZombieWorkerConfig) ?event_loop.ZombieSession {
    const session = event_loop.claimZombie(alloc, cfg.zombie_id, cfg.pool) catch |err| {
        obs_log.logErrWithHint(.zombie_worker, err, error_codes.ERR_ZOMBIE_CLAIM_FAILED, "zombie_worker.claim_fail zombie_id={s}", .{cfg.zombie_id});
        return null;
    };
    log.info("zombie_worker.claimed zombie_id={s} name={s}", .{ cfg.zombie_id, session.config.name });
    return session;
}

/// Query core.zombies for active zombies and return their IDs.
/// Called at worker startup to discover which Zombies to claim.
pub fn listActiveZombieIds(pool: *pg.Pool, alloc: std.mem.Allocator) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = try conn.query( // check-pg-drain: ok — drain called below
        \\SELECT id::text FROM core.zombies
        \\WHERE status = $1
        \\ORDER BY created_at ASC
    , .{error_codes.ZOMBIE_STATUS_ACTIVE});
    defer q.deinit();

    var ids: std.ArrayList([]const u8) = .{};
    errdefer {
        for (ids.items) |id| alloc.free(id);
        ids.deinit(alloc);
    }
    while (try q.*.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        try ids.append(alloc, id);
    }
    q.drain() catch {};
    return ids.toOwnedSlice(alloc);
}

/// Polls shutdown_requested and sets running=false when shutdown is triggered.
fn watchShutdown(shutdown: *const std.atomic.Value(bool), running: *std.atomic.Value(bool)) void {
    while (!shutdown.load(.acquire) and running.load(.acquire)) {
        std.Thread.sleep(100 * std.time.ns_per_ms);
    }
    running.store(false, .release);
}

fn connectRedis(alloc: std.mem.Allocator) ?queue_redis.Client {
    return queue_redis.Client.connectFromEnv(alloc, .worker) catch |err| {
        obs_log.logErrWithHint(
            .zombie_worker,
            err,
            error_codes.ERR_STARTUP_REDIS_CONNECT,
            "zombie_worker.redis_unavailable",
            .{},
        );
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
