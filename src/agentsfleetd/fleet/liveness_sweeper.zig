//! Runner liveness sweeper.

const std = @import("std");
const sql = @import("sql.zig");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const protocol = @import("contract").protocol;
const runner_events = @import("runner_events.zig");
const tripwire = @import("tripwire");

const log = logging.scoped(.runner_liveness_sweeper);

const SWEEP_BATCH_LIMIT: i64 = 100;
const EXPIRE_PAST_DELTA_MS: i64 = 1;
const SWEEP_INTERVAL_NS: u64 = @as(u64, @intCast(constants.HEARTBEAT_INTERVAL_MS)) * std.time.ns_per_ms;
const SHUTDOWN_POLL_NS: u64 = std.time.ns_per_s;
const SQL_BEGIN = "BEGIN";
const SQL_COMMIT = "COMMIT";
const LOG_SWEEPER_STARTED = "sweeper_started";
const LOG_SWEEPER_STOPPED = "sweeper_stopped";
const LOG_SWEEP_FAILED = "sweep_failed";
const LOG_SWEEP_COMPLETED = "sweep_completed";
const LOG_ROLLBACK_FAILED = "rollback_failed";

const RunnerRef = struct {
    id: []const u8,
    last_seen_at: i64,
    admin_state: protocol.AdminState,
};

/// Fail points for `fetchDueRunners`' errdefer ladder. Public so the sibling
/// integration test can arm them; comptime-erased outside test builds.
pub const fetch_tw = tripwire.module(enum {
    row_state,
    dupe_id,
    append_ref,
    to_owned,
}, error{ OutOfMemory, DbRowShape });

const OfflineSweep = struct {
    inserted_event: bool = false,
    expired_slots: i64 = 0,
};

pub const SweepStats = struct {
    scanned_runners: i64 = 0,
    offline_events: i64 = 0,
    expired_slots: i64 = 0,
    drained_runners: i64 = 0,
};

/// Run until shutdown is signalled. Spawned by the serve lifecycle.
pub fn run(pool: *pg.Pool, alloc: std.mem.Allocator, shutdown: *std.atomic.Value(bool)) void {
    log.debug(LOG_SWEEPER_STARTED, .{ .interval_ms = constants.HEARTBEAT_INTERVAL_MS, .batch_limit = SWEEP_BATCH_LIMIT });
    while (!shutdown.load(.acquire)) { // safe because: pairs with serve_shutdown's background-stop release-store (watcher server-stop / teardown disarm).
        const stats = sweepOnce(pool, alloc) catch |err| {
            log.warn(LOG_SWEEP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
            continue;
        };
        if (stats.scanned_runners > 0) log.debug(LOG_SWEEP_COMPLETED, .{
            .scanned_runners = stats.scanned_runners,
            .offline_events = stats.offline_events,
            .expired_slots = stats.expired_slots,
            .drained_runners = stats.drained_runners,
        });
        sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
    }
    log.debug(LOG_SWEEPER_STOPPED, .{});
}

/// Execute one bounded sweep. Tests call this directly.
pub fn sweepOnce(pool: *pg.Pool, alloc: std.mem.Allocator) !SweepStats {
    const now_ms = clock.nowMillis();
    const runners = try fetchDueRunners(pool, alloc, now_ms);
    defer freeRunnerRefs(alloc, runners);
    var stats = SweepStats{ .scanned_runners = @intCast(runners.len) };
    for (runners) |runner| try sweepRunner(pool, alloc, runner, now_ms, &stats);
    return stats;
}

fn sweepRunner(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    runner: RunnerRef,
    now_ms: i64,
    stats: *SweepStats,
) !void {
    if (isStale(runner.last_seen_at, now_ms)) {
        const offline = try sweepOfflineRunner(pool, alloc, runner.id, runner.last_seen_at, now_ms);
        if (offline.inserted_event) stats.offline_events += 1;
        stats.expired_slots += offline.expired_slots;
    }
    if (runner.admin_state != .active) stats.expired_slots += try expireRunnerSlots(pool, runner.id, now_ms);
    if (runner.admin_state == .draining and try markDrainedIfIdle(pool, alloc, runner.id, now_ms)) {
        stats.drained_runners += 1;
    }
}

fn fetchDueRunners(pool: *pg.Pool, alloc: std.mem.Allocator, now_ms: i64) ![]RunnerRef {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(sql.SELECT_DUE_RUNNERS, .{
        protocol.RUNNER_LAST_SEEN_NEVER,
        now_ms,
        constants.RUNNER_OFFLINE_AFTER_MS,
        protocol.ADMIN_STATE_ACTIVE,
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        @tagName(protocol.AdminState.draining),
        SWEEP_BATCH_LIMIT,
    }));
    defer q.deinit();

    var refs: std.ArrayList(RunnerRef) = .empty;
    errdefer {
        freeRunnerRefItems(alloc, refs.items);
        refs.deinit(alloc);
    }
    while (try q.next()) |row| {
        try fetch_tw.check(.row_state);
        const raw_state = try row.get([]const u8, 2);
        const admin_state = std.meta.stringToEnum(protocol.AdminState, raw_state) orelse return error.DbRowShape;
        // Fallible row reads land in locals BEFORE the dupe: Zig does not
        // unwind earlier struct-literal fields when a later one errors, so no
        // owned allocation may be in flight while another field can fail.
        const last_seen_at = try row.get(i64, 1);
        try fetch_tw.check(.dupe_id);
        const id = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(id);
        try fetch_tw.check(.append_ref);
        try refs.append(alloc, .{
            .id = id,
            .last_seen_at = last_seen_at,
            .admin_state = admin_state,
        });
    }
    try fetch_tw.check(.to_owned);
    return refs.toOwnedSlice(alloc);
}

fn sweepOfflineRunner(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    last_seen_at: i64,
    now_ms: i64,
) !OfflineSweep {
    const conn = try pool.acquire();
    defer pool.release(conn);
    _ = try conn.exec(SQL_BEGIN, .{});
    var tx_open = true;
    errdefer if (tx_open) rollback(conn);

    const inserted = try insertOfflineEvent(conn, alloc, runner_id, last_seen_at, now_ms);
    const expired = if (inserted) try expireActiveLeaseSlots(conn, runner_id, now_ms) else 0;
    _ = try conn.exec(SQL_COMMIT, .{});
    tx_open = false;
    return .{ .inserted_event = inserted, .expired_slots = expired };
}

fn insertOfflineEvent(
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    runner_id: []const u8,
    last_seen_at: i64,
    now_ms: i64,
) !bool {
    const event_row_id = try id_format.generateRunnerEventId(alloc);
    defer alloc.free(event_row_id);
    var q = PgQuery.from(try conn.query(sql.INSERT_OFFLINE_EVENT, .{
        event_row_id,
        runner_id,
        @tagName(protocol.RunnerEventType.runner_offline),
        now_ms,
        runner_events.META_LAST_SEEN_AT,
        last_seen_at,
    }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return (try row.get(i64, 0)) == 1;
}

fn expireRunnerSlots(pool: *pg.Pool, runner_id: []const u8, now_ms: i64) !i64 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    return expireActiveLeaseSlots(conn, runner_id, now_ms);
}

fn expireActiveLeaseSlots(conn: *pg.Conn, runner_id: []const u8, now_ms: i64) !i64 {
    const expire_at = now_ms - EXPIRE_PAST_DELTA_MS;
    var q = PgQuery.from(try conn.query(sql.EXPIRE_ACTIVE_LEASE_SLOTS, .{ runner_id, protocol.RUNNER_LEASE_STATUS_ACTIVE, expire_at, now_ms }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return row.get(i64, 0);
}

fn markDrainedIfIdle(pool: *pg.Pool, alloc: std.mem.Allocator, runner_id: []const u8, now_ms: i64) !bool {
    const event_row_id = try id_format.generateRunnerEventId(alloc);
    defer alloc.free(event_row_id);
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(sql.MARK_DRAINED_IF_IDLE, .{
        runner_id,
        @tagName(protocol.AdminState.drained),
        now_ms,
        @tagName(protocol.AdminState.draining),
        protocol.RUNNER_LEASE_STATUS_ACTIVE,
        event_row_id,
        @tagName(protocol.RunnerEventType.runner_drained),
        runner_events.META_FROM_ADMIN_STATE,
        runner_events.META_TO_ADMIN_STATE,
    }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.DbRowShape;
    return (try row.get(i64, 0)) == 1;
}

fn isStale(last_seen_at: i64, now_ms: i64) bool {
    return last_seen_at != protocol.RUNNER_LAST_SEEN_NEVER and
        now_ms - last_seen_at > constants.RUNNER_OFFLINE_AFTER_MS;
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

fn rollback(conn: *pg.Conn) void {
    conn.rollback() catch |err| log.warn(LOG_ROLLBACK_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
}

fn freeRunnerRefs(alloc: std.mem.Allocator, refs: []RunnerRef) void {
    freeRunnerRefItems(alloc, refs);
    alloc.free(refs);
}

fn freeRunnerRefItems(alloc: std.mem.Allocator, refs: []RunnerRef) void {
    for (refs) |ref| alloc.free(ref.id);
}
