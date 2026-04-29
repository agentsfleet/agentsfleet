// Approval gate auto-timeout sweeper.
//
// Background thread that walks core.zombie_approval_gates every SCAN_INTERVAL,
// transitioning rows whose timeout_at has passed from `pending` to `timed_out`
// via the channel-agnostic resolve core. The DB UPDATE precondition + Redis
// decision write happen atomically, so worker threads blocked on the gate
// observe the timeout via their existing waitForDecision poll within one cycle.
//
// Resolver attribution is `system:timeout` so the dashboard can distinguish
// auto-denials from operator-initiated denials. Worker treats `.timed_out`
// the same as `.denied` (safe default for destructive ops).

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const queue_redis = @import("../queue/redis_client.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const approval_gate = @import("approval_gate.zig");
const resolver = @import("approval_gate_resolver.zig");

const log = std.log.scoped(.approval_gate_sweeper);

const PENDING_STATUS = approval_gate.GateStatus.pending.toSlice();

const SCAN_INTERVAL_NS: u64 = 60 * std.time.ns_per_s;
const SHUTDOWN_POLL_NS: u64 = 1 * std.time.ns_per_s;
const BATCH_LIMIT: u32 = 100;
const RESOLVER = resolver.SYSTEM_TIMEOUT;

/// Run the sweeper loop until shutdown is signalled. Intended to be spawned
/// in its own thread by the worker process at boot time.
pub fn run(
    pool: *pg.Pool,
    redis: *queue_redis.Client,
    alloc: Allocator,
    shutdown: *std.atomic.Value(bool),
) void {
    log.info("approval_gate_sweeper.started interval_s={d} batch_limit={d}", .{ SCAN_INTERVAL_NS / std.time.ns_per_s, BATCH_LIMIT });
    while (!shutdown.load(.acquire)) {
        sweepOnce(pool, redis, alloc) catch |err| {
            log.warn("approval_gate_sweeper.sweep_failed err={s}", .{@errorName(err)});
        };
        sleepInterruptible(shutdown, SCAN_INTERVAL_NS);
    }
    log.info("approval_gate_sweeper.shutdown", .{});
}

fn sweepOnce(pool: *pg.Pool, redis: *queue_redis.Client, alloc: Allocator) !void {
    const expired = try fetchExpired(pool, alloc);
    defer freeExpired(alloc, expired);

    if (expired.len == 0) return;

    log.info("approval_gate_sweeper.expired_batch count={d}", .{expired.len});
    for (expired) |action_id| {
        var outcome = approval_gate.resolve(pool, redis, alloc, action_id, .timed_out, RESOLVER, "auto-timeout") catch |err| {
            log.warn("approval_gate_sweeper.resolve_failed action_id={s} err={s}", .{ action_id, @errorName(err) });
            continue;
        };
        defer switch (outcome) {
            .resolved => |*r| @constCast(r).deinit(alloc),
            .already_resolved => |*r| @constCast(r).deinit(alloc),
            .not_found => {},
        };
        switch (outcome) {
            .resolved => log.info("approval_gate_sweeper.timed_out action_id={s}", .{action_id}),
            .already_resolved => |r| log.debug("approval_gate_sweeper.race_lost action_id={s} winning_outcome={s} winning_by={s}", .{ action_id, r.outcome.toSlice(), r.resolved_by }),
            .not_found => log.debug("approval_gate_sweeper.row_disappeared action_id={s}", .{action_id}),
        }
    }
}

fn fetchExpired(pool: *pg.Pool, alloc: Allocator) ![][]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    var q = PgQuery.from(try conn.query(
        \\SELECT action_id FROM core.zombie_approval_gates
        \\WHERE status = $1 AND timeout_at <= $2
        \\ORDER BY timeout_at ASC
        \\LIMIT $3
    , .{ PENDING_STATUS, now_ms, @as(i64, @intCast(BATCH_LIMIT)) }));
    defer q.deinit();

    var ids: std.ArrayList([]const u8) = .{};
    errdefer freeExpiredArrayList(alloc, &ids);

    while (try q.next()) |row| {
        const raw = try row.get([]const u8, 0);
        const owned = try alloc.dupe(u8, raw);
        errdefer alloc.free(owned);
        try ids.append(alloc, owned);
    }

    return try ids.toOwnedSlice(alloc);
}

fn freeExpired(alloc: Allocator, ids: [][]const u8) void {
    for (ids) |id| alloc.free(id);
    alloc.free(ids);
}

fn freeExpiredArrayList(alloc: Allocator, ids: *std.ArrayList([]const u8)) void {
    for (ids.items) |id| alloc.free(id);
    ids.deinit(alloc);
}

/// Sleep up to `total_ns` but wake early if shutdown is signalled. Polls the
/// flag every SHUTDOWN_POLL_NS so a SIGTERM doesn't have to wait a full cycle.
fn sleepInterruptible(shutdown: *std.atomic.Value(bool), total_ns: u64) void {
    var remaining: u64 = total_ns;
    while (remaining > 0) {
        if (shutdown.load(.acquire)) return;
        const step = @min(remaining, SHUTDOWN_POLL_NS);
        std.Thread.sleep(step);
        remaining -|= step;
    }
}
