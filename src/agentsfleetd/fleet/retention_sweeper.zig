//! Runner retention sweeper.
//!
//! `fleet.runner_leases` and `fleet.runner_events` gain rows on every claim
//! and nothing pruned them — the operator read paths were sized by a runner's
//! whole life. Lifetime tallies now live in `fleet.runner_lifetime_counters`
//! (maintained by the lease write paths), so terminal rows past the retention
//! window carry no signal the operator surface still reads: this sweeper
//! deletes them in bounded batches. Live work is untouchable by construction —
//! the lease predicate admits terminal statuses only, and event rows are
//! pruned by age alone (their lease context is already terminal or gone).

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const protocol = @import("contract").protocol;
const metrics = @import("../observability/metrics_counters.zig");

const log = logging.scoped(.runner_retention_sweeper);

/// Terminal rows older than this are eligible. Lifetime counters make the
/// pruned history redundant for the operator surface; 30 days keeps a month of
/// row-level forensics.
const RETENTION_WINDOW_MS: i64 = 30 * std.time.ms_per_day;
/// One batched DELETE's ceiling — bounds lock time and WAL per statement.
const DELETE_BATCH_LIMIT: i64 = 1000;
/// Batches per table per cycle — bounds a cycle's total work; the backlog
/// drains across cycles rather than monopolizing a connection.
const MAX_BATCHES_PER_CYCLE: usize = 8;
const SWEEP_INTERVAL_NS: u64 = std.time.ns_per_hour;
const SHUTDOWN_POLL_NS: u64 = std.time.ns_per_s;

const LOG_SWEEPER_STARTED = "sweeper_started";
const LOG_SWEEPER_STOPPED = "sweeper_stopped";
const LOG_SWEEP_FAILED = "sweep_failed";
const LOG_SWEEP_COMPLETED = "sweep_completed";

/// Keyed on `id` (UNIQUE) so the outer DELETE stays a PK-shaped semi-join;
/// the inner SELECT walks the terminal/aged subset only.
const DELETE_TERMINAL_LEASES_BATCH =
    \\DELETE FROM fleet.runner_leases
    \\WHERE id IN (
    \\  SELECT id FROM fleet.runner_leases
    \\  WHERE status = ANY($1::text[]) AND created_at < $2
    \\  LIMIT $3
    \\)
;

const DELETE_AGED_RUNNER_EVENTS_BATCH =
    \\DELETE FROM fleet.runner_events
    \\WHERE id IN (
    \\  SELECT id FROM fleet.runner_events
    \\  WHERE occurred_at < $1
    \\  LIMIT $2
    \\)
;

pub const SweepTotals = struct {
    leases_deleted: i64 = 0,
    events_deleted: i64 = 0,
};

/// Run until shutdown is signalled. Spawned by the serve lifecycle.
pub fn run(pool: *pg.Pool, shutdown: *std.atomic.Value(bool)) void {
    log.debug(LOG_SWEEPER_STARTED, .{ .interval_ms = SWEEP_INTERVAL_NS / std.time.ns_per_ms, .window_ms = RETENTION_WINDOW_MS });
    while (!shutdown.load(.acquire)) { // safe because: pairs with serve_shutdown's background-stop release-store.
        const totals = sweepOnce(pool) catch |err| {
            log.warn(LOG_SWEEP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
            continue;
        };
        const swept = totals.leases_deleted + totals.events_deleted;
        if (swept > 0) {
            metrics.addRetentionSwept(@intCast(swept));
            log.info(LOG_SWEEP_COMPLETED, .{
                .leases_deleted = totals.leases_deleted,
                .events_deleted = totals.events_deleted,
            });
        }
        sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
    }
    log.debug(LOG_SWEEPER_STOPPED, .{});
}

/// Execute one bounded sweep cycle. Tests call this directly.
pub fn sweepOnce(pool: *pg.Pool) !SweepTotals {
    const now_ms = clock.nowMillis();
    const cutoff = now_ms - RETENTION_WINDOW_MS;
    const terminal = [_][]const u8{
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
    };
    const conn = try pool.acquire();
    defer pool.release(conn);

    var totals = SweepTotals{};
    var batch: usize = 0;
    while (batch < MAX_BATCHES_PER_CYCLE) : (batch += 1) {
        const deleted = (try conn.exec(DELETE_TERMINAL_LEASES_BATCH, .{ &terminal, cutoff, DELETE_BATCH_LIMIT })) orelse 0;
        totals.leases_deleted += deleted;
        if (deleted < DELETE_BATCH_LIMIT) break;
    }
    batch = 0;
    while (batch < MAX_BATCHES_PER_CYCLE) : (batch += 1) {
        const deleted = (try conn.exec(DELETE_AGED_RUNNER_EVENTS_BATCH, .{ cutoff, DELETE_BATCH_LIMIT })) orelse 0;
        totals.events_deleted += deleted;
        if (deleted < DELETE_BATCH_LIMIT) break;
    }
    return totals;
}

/// Sleep the interval in shutdown-poll slices so a stop request never waits
/// out the full hour.
fn sleepInterruptible(shutdown: *std.atomic.Value(bool), total_ns: u64) void {
    var remaining = total_ns;
    while (remaining > 0) {
        if (shutdown.load(.acquire)) return; // safe because: same pairing as the run loop's check.
        const step = @min(remaining, SHUTDOWN_POLL_NS);
        constants.sleepNanos(step);
        remaining -|= step;
    }
}
