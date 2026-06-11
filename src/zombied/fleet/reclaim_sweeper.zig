//! Stranded-delivery reclaim sweeper.
//!
//! Entries delivered to a consumer that no longer reads — a retired zombied
//! instance, the legacy per-probe `worker-{host}-{ts}` names — strand in that
//! consumer's Pending Entries List (PEL) forever: `XREADGROUP ">"` never
//! re-delivers them. This sweep XAUTOCLAIMs entries idle past the
//! comptime-bounded min-idle into THIS instance's stable consumer, where the
//! lease path's own-PEL read (`assign.acquireFresh`) re-enters them into the
//! lease flow on the next poll. Live work is never raced: the min-idle exceeds
//! the lease window (comptime assertion in `queue/constants.zig`) and the
//! lease path re-checks the per-zombie affinity claim before any re-delivery.
//!
//! Loop shape mirrors `liveness_sweeper`: bounded batch, interruptible sleep,
//! joined by `serve_background.Threads.stop`.

const std = @import("std");
const constants = @import("common");
const logging = @import("log");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const queue_consts = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const zombie_config = @import("../zombie/config.zig");

const log = logging.scoped(.reclaim_sweeper);

const SWEEP_BATCH_LIMIT: i64 = 100;
/// Per-zombie per-sweep claim bound — keeps one pathological stream from
/// monopolizing a sweep pass; the next pass continues where this one stopped.
const SWEEP_CLAIM_LIMIT: usize = 10;
const SWEEP_INTERVAL_NS: u64 = @as(u64, @intCast(queue_consts.zombie_reclaim_interval_ms)) * std.time.ns_per_ms;
const SHUTDOWN_POLL_NS: u64 = std.time.ns_per_s;
const LOG_SWEEPER_STARTED = "sweeper_started";
const LOG_SWEEPER_STOPPED = "sweeper_stopped";
const LOG_SWEEP_FAILED = "sweep_failed";

pub const SweepStats = struct {
    scanned_zombies: i64 = 0,
    reclaimed_entries: i64 = 0,
};

/// Run until shutdown is signalled. Spawned by the serve lifecycle.
pub fn run(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, shutdown: *std.atomic.Value(bool)) void {
    log.info(LOG_SWEEPER_STARTED, .{ .interval_ms = queue_consts.zombie_reclaim_interval_ms, .min_idle_ms = queue_consts.zombie_xautoclaim_min_idle_ms_int, .batch_limit = SWEEP_BATCH_LIMIT });
    while (!shutdown.load(.acquire)) { // safe because: pairs with serve_shutdown.request() release-store.
        const stats = sweepOnce(pool, queue, alloc) catch |err| {
            log.warn(LOG_SWEEP_FAILED, .{ .err = @errorName(err) });
            sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
            continue;
        };
        if (stats.reclaimed_entries > 0) log.info("sweep_completed", .{
            .scanned_zombies = stats.scanned_zombies,
            .reclaimed_entries = stats.reclaimed_entries,
        });
        sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
    }
    log.info(LOG_SWEEPER_STOPPED, .{});
}

/// Execute one bounded sweep. Tests call this directly.
pub fn sweepOnce(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator) !SweepStats {
    const zombies = try fetchActiveZombies(pool, alloc);
    defer freeIds(alloc, zombies);
    var consumer_buf: [queue_redis.CONSUMER_ID_BUF_LEN]u8 = undefined;
    const consumer_id = queue_redis.stableConsumerId(&consumer_buf);
    var stats = SweepStats{ .scanned_zombies = @intCast(zombies.len) };
    for (zombies) |zombie_id| {
        stats.reclaimed_entries += reclaimZombieStrays(queue, zombie_id, consumer_id);
    }
    return stats;
}

/// Claim up to SWEEP_CLAIM_LIMIT idle-past-bound entries for one zombie into
/// the stable consumer, logging each (RULE OBS). XAUTOCLAIM resets the
/// claimed entry's idle clock, so the loop terminates: a re-encountered entry
/// is no longer eligible. Redis errors collapse to "claimed nothing" — the
/// next pass retries.
fn reclaimZombieStrays(queue: *queue_redis.Client, zombie_id: []const u8, consumer_id: []const u8) i64 {
    var reclaimed: i64 = 0;
    var i: usize = 0;
    while (i < SWEEP_CLAIM_LIMIT) : (i += 1) {
        var event = (redis_zombie.xautoclaimZombie(queue, zombie_id, consumer_id) catch |err| {
            log.warn("reclaim_claim_failed", .{ .zombie_id = zombie_id, .err = @errorName(err) });
            return reclaimed;
        }) orelse return reclaimed;
        defer event.deinit(queue.alloc);
        reclaimed += 1;
        log.info("reclaim_swept", .{ .zombie_id = zombie_id, .event_id = event.event_id, .actor = event.actor });
    }
    return reclaimed;
}

/// Active zombies only: a paused/stopped zombie's entries are deliberately
/// retained where they are — on resume the zombie re-enters the candidate
/// scan and this sweep picks its strays up on the next pass.
fn fetchActiveZombies(pool: *pg.Pool, alloc: std.mem.Allocator) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text FROM core.zombies WHERE status = $1
        \\ORDER BY updated_at ASC LIMIT $2
    , .{ zombie_config.ZombieStatus.active.toSlice(), SWEEP_BATCH_LIMIT }));
    defer q.deinit();
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer freeIdItems(alloc, ids.items);
    while (try q.next()) |row| {
        try ids.append(alloc, try alloc.dupe(u8, try row.get([]const u8, 0)));
    }
    return ids.toOwnedSlice(alloc);
}

fn sleepInterruptible(shutdown: *std.atomic.Value(bool), total_ns: u64) void {
    var remaining = total_ns;
    while (remaining > 0) {
        if (shutdown.load(.acquire)) return; // safe because: pairs with serve_shutdown.request() release-store.
        const step = @min(remaining, SHUTDOWN_POLL_NS);
        constants.sleepNanos(step);
        remaining -|= step;
    }
}

fn freeIds(alloc: std.mem.Allocator, ids: [][]const u8) void {
    freeIdItems(alloc, ids);
    alloc.free(ids);
}

fn freeIdItems(alloc: std.mem.Allocator, ids: [][]const u8) void {
    for (ids) |id| alloc.free(id);
}
