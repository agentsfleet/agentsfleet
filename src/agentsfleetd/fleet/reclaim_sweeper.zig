//! Stranded-delivery reclaim sweeper, and the readiness index's backstop.
//!
//! Entries delivered to a consumer that no longer reads — a retired agentsfleetd
//! instance, the legacy per-probe `worker-{host}-{ts}` names — strand in that
//! consumer's Pending Entries List (PEL) forever: `XREADGROUP ">"` never
//! re-delivers them. This sweep XAUTOCLAIMs entries idle past the
//! comptime-bounded min-idle into THIS instance's stable consumer, where the
//! lease path's own-PEL read (`assign.acquireFresh`) re-enters them into the
//! lease flow on the next poll. Live work is never raced: the min-idle exceeds
//! the lease window (comptime assertion in `queue/constants.zig`) and the
//! lease path re-checks the per-fleet affinity claim before any re-delivery.
//!
//! **It is also what keeps a lost readiness mark from becoming lost work.** The
//! readiness index is a hint; the streams are the system of record. Each pass
//! re-marks any fleet still holding deliverable work, so an ingress mark that
//! failed, or an index that was evicted or flushed, self-heals here. The sweep
//! only ever re-marks and never clears: a false positive costs one wasted
//! candidate check, a false negative strands an event.
//!
//! **Recovery is bounded by fleet count, not by a flat interval.** A pass reaches
//! at most `SWEEP_BATCH_LIMIT` active fleets, so the worst case for a strand
//! outside the current batch is the auto-claim min-idle plus
//! `ceil(active_fleets / SWEEP_BATCH_LIMIT)` intervals. The keyset cursor below
//! is what makes that bound finite at all — the previous ordered-and-limited form
//! re-read the same head of the list every pass and never reached the remainder.
//!
//! Loop shape mirrors `liveness_sweeper`: bounded batch, interruptible sleep,
//! joined by `serve_background.Threads.stop`.

const std = @import("std");
const constants = @import("common");
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const queue_consts = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis_client.zig");
const redis_fleet = @import("../queue/redis_fleet.zig");
const fleet_probe = @import("../queue/redis_fleet_probe.zig");
const fleet_ready = @import("../queue/fleet_ready.zig");
const metrics = @import("../observability/metrics_counters.zig");
const id_format = @import("../types/id_format.zig");
const fleet_config = @import("../fleet_runtime/config.zig");

const log = logging.scoped(.reclaim_sweeper);

const SWEEP_BATCH_LIMIT: i64 = 100;
/// Per-fleet per-sweep claim bound — keeps one pathological stream from
/// monopolizing a sweep pass; the next pass continues where this one stopped.
const SWEEP_CLAIM_LIMIT: usize = 10;
const SWEEP_INTERVAL_NS: u64 = @as(u64, @intCast(queue_consts.fleet_reclaim_interval_ms)) * std.time.ns_per_ms;
const SHUTDOWN_POLL_NS: u64 = std.time.ns_per_s;
const LOG_SWEEPER_STARTED = "sweeper_started";
const LOG_SWEEPER_STOPPED = "sweeper_stopped";
const LOG_SWEEP_FAILED = "sweep_failed";
/// Keyset floor for the first pass of a cycle: lower than every real
/// `(updated_at, id)` pair. Never stored — only compared against.
const KEYSET_START_UPDATED_AT: i64 = 0;
const KEYSET_START_ID = "00000000-0000-0000-0000-000000000000";

pub const SweepStats = struct {
    scanned_agents: i64 = 0,
    reclaimed_entries: i64 = 0,
    remarked_fleets: i64 = 0,
};

/// Where the previous pass stopped, so successive passes advance through the
/// active-fleet population instead of re-reading its head.
///
/// Only touched by the sweeper thread (`run` owns the single instance), so it
/// needs no synchronisation — `sweepOnce` takes it by pointer so a test can drive
/// several passes against one cursor and observe the advance.
pub const Cursor = struct {
    after_updated_at: i64 = KEYSET_START_UPDATED_AT,
    after_id_buf: [id_format.UUID_TEXT_LEN]u8 = [_]u8{0} ** id_format.UUID_TEXT_LEN,
    after_id_len: usize = 0,

    fn afterId(self: *const Cursor) []const u8 {
        if (self.after_id_len == 0) return KEYSET_START_ID;
        return self.after_id_buf[0..self.after_id_len];
    }

    /// Copies `id` in rather than borrowing it: the caller's slice is row-backed
    /// and dies at the query's `deinit`, while the cursor outlives the pass.
    fn advance(self: *Cursor, updated_at: i64, id: []const u8) void {
        const len = @min(id.len, self.after_id_buf.len);
        @memcpy(self.after_id_buf[0..len], id[0..len]);
        self.after_id_len = len;
        self.after_updated_at = updated_at;
    }

    /// Back to the start of the population. Called when a pass returns a short
    /// batch, which means the cursor reached the end of the active set.
    fn rewind(self: *Cursor) void {
        self.after_updated_at = KEYSET_START_UPDATED_AT;
        self.after_id_len = 0;
    }
};

/// Run until shutdown is signalled. Spawned by the serve lifecycle.
pub fn run(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, shutdown: *std.atomic.Value(bool)) void {
    log.debug(LOG_SWEEPER_STARTED, .{ .interval_ms = queue_consts.fleet_reclaim_interval_ms, .min_idle_ms = queue_consts.fleet_xautoclaim_min_idle_ms_int, .batch_limit = SWEEP_BATCH_LIMIT });
    var cursor = Cursor{}; // only touched by this thread
    while (!shutdown.load(.acquire)) { // safe because: pairs with serve_shutdown's background-stop release-store (watcher server-stop / teardown disarm).
        const stats = sweepOnce(pool, queue, alloc, &cursor) catch |err| {
            log.warn(LOG_SWEEP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
            continue;
        };
        if (stats.reclaimed_entries > 0 or stats.remarked_fleets > 0) log.debug("sweep_completed", .{
            .scanned_agents = stats.scanned_agents,
            .reclaimed_entries = stats.reclaimed_entries,
            .remarked_fleets = stats.remarked_fleets,
        });
        sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
    }
    log.debug(LOG_SWEEPER_STOPPED, .{});
}

/// Execute one bounded sweep, advancing `cursor`. Tests call this directly.
pub fn sweepOnce(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, cursor: *Cursor) !SweepStats {
    const fleets = try fetchActiveFleets(pool, alloc, cursor);
    defer freeIds(alloc, fleets);
    var consumer_buf: [queue_redis.CONSUMER_ID_BUF_LEN]u8 = undefined;
    const consumer_id = queue_redis.stableConsumerId(&consumer_buf);
    var stats = SweepStats{ .scanned_agents = @intCast(fleets.len) };
    for (fleets) |fleet_id| {
        const reclaimed = reclaimFleetStrays(queue, fleet_id, consumer_id);
        stats.reclaimed_entries += reclaimed;
        if (remarkIfDeliverable(queue, fleet_id, reclaimed > 0)) stats.remarked_fleets += 1;
    }
    sampleReadyDepth(queue);
    return stats;
}

/// Re-mark `fleet_id` when it still holds work a runner could pick up.
///
/// `reclaimed_any` short-circuits the probe: an entry we just claimed into this
/// instance's PEL is deliverable by definition, so there is nothing to ask Redis.
/// This is also the half `XAUTOCLAIM` alone can never cover — an appended entry
/// whose readiness mark then failed sits in nobody's pending list, so only the
/// undelivered probe finds it.
fn remarkIfDeliverable(queue: *queue_redis.Client, fleet_id: []const u8, reclaimed_any: bool) bool {
    if (!reclaimed_any and !fleet_probe.hasDeliverable(queue, fleet_id)) return false;
    fleet_ready.mark(queue, fleet_id);
    log.debug("ready_remarked", .{ .fleet_id = fleet_id, .after_reclaim = reclaimed_any });
    return true;
}

/// Sample the shared index's field count into the metrics registry.
///
/// Sampled here rather than counted at mark/clear because the index is ONE hash
/// shared by every replica: one replica marks while another clears, a restart
/// zeroes any local delta, and a repeat mark for an already-present fleet changes
/// no field count. Reading it once per pass also keeps `/metrics` free of Redis.
fn sampleReadyDepth(queue: *queue_redis.Client) void {
    const fields = fleet_ready.depth(queue) catch |err| {
        log.warn("ready_depth_sample_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return;
    };
    metrics.setReadyIndexDepth(fields);
}

/// Claim up to SWEEP_CLAIM_LIMIT idle-past-bound entries for one fleet into
/// the stable consumer, logging each (RULE OBS). XAUTOCLAIM resets the
/// claimed entry's idle clock, so the loop terminates: a re-encountered entry
/// is no longer eligible. Redis errors collapse to "claimed nothing" — the
/// next pass retries.
fn reclaimFleetStrays(queue: *queue_redis.Client, fleet_id: []const u8, consumer_id: []const u8) i64 {
    var reclaimed: i64 = 0;
    var i: usize = 0;
    while (i < SWEEP_CLAIM_LIMIT) : (i += 1) {
        var event = (redis_fleet.xautoclaimFleet(queue, fleet_id, consumer_id) catch |err| {
            log.warn("reclaim_claim_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
            return reclaimed;
        }) orelse return reclaimed;
        defer event.deinit(queue.alloc);
        reclaimed += 1;
        log.debug("reclaim_swept", .{ .fleet_id = fleet_id, .event_id = event.event_id, .actor = event.actor });
    }
    return reclaimed;
}

/// The next batch of active fleets after `cursor`, advancing it.
///
/// Active fleets only: a paused/stopped fleet's entries are deliberately
/// retained where they are — on resume the fleet re-enters the candidate scan and
/// this sweep picks its strays up on the next pass.
///
/// Keyset-paged on the composite `(updated_at, id)` rather than offset-paged, so
/// a fleet whose `updated_at` changes mid-cycle cannot cause the scan to skip or
/// repeat its neighbours (RULE KYS). A short batch means the end of the population
/// was reached, so the cursor rewinds and the next pass starts a fresh cycle.
fn fetchActiveFleets(pool: *pg.Pool, alloc: std.mem.Allocator, cursor: *Cursor) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT id::text, updated_at FROM core.fleets
        \\WHERE status = $1 AND (updated_at, id) > ($2::bigint, $3::uuid)
        \\ORDER BY updated_at ASC, id ASC LIMIT $4
    , .{ fleet_config.FleetStatus.active.toSlice(), cursor.after_updated_at, cursor.afterId(), SWEEP_BATCH_LIMIT }));
    defer q.deinit();
    var ids: std.ArrayList([]const u8) = .empty;
    errdefer {
        freeIdItems(alloc, ids.items);
        ids.deinit(alloc);
    }
    var last_updated_at: i64 = cursor.after_updated_at;
    while (try q.next()) |row| {
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        last_updated_at = try row.get(i64, 1);
        try ids.append(alloc, id);
    }
    if (ids.items.len < SWEEP_BATCH_LIMIT) {
        cursor.rewind();
    } else {
        cursor.advance(last_updated_at, ids.items[ids.items.len - 1]);
    }
    return ids.toOwnedSlice(alloc);
}

fn sleepInterruptible(shutdown: *std.atomic.Value(bool), total_ns: u64) void {
    var remaining = total_ns;
    while (remaining > 0) {
        if (shutdown.load(.acquire)) return; // safe because: pairs with serve_shutdown's background-stop release-store (watcher server-stop / teardown disarm).
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
