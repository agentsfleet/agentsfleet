//! Runner retention sweeper.
//!
//! `fleet.runner_leases` and `fleet.runner_events` gain rows on every claim
//! and nothing pruned them — the operator read paths were sized by a runner's
//! whole life. Lifetime tallies now live in `fleet.runner_lifetime_counters`
//! (maintained by the lease write paths), so terminal rows past the retention
//! window carry no signal the operator surface still reads: this sweeper
//! deletes them in bounded batches.
//!
//! What the window measures, and what it spares:
//!
//!  * Leases age from `updated_at`, the instant settle or reclaim moved the row
//!    to its terminal status — NOT from `created_at`. Retention is a promise
//!    about how long a *settled* lease stays readable, so a lease acquired long
//!    ago and settled yesterday keeps its full window.
//!  * Events age from `occurred_at`, and only the per-work tags are eligible.
//!    The lifecycle tags are the Activity feed's entire content and are kept:
//!    pruning them by age blanked the feed for every runner enrolled before the
//!    window, which is exactly the operator surface this sweeper exists to keep
//!    fast.
//!  * Live work is untouchable by construction: the lease predicate admits
//!    terminal statuses only, and the comptime check below proves no live
//!    lease can reach the age cutoff, so no live lease's events can either.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const protocol = @import("contract").protocol;
const metrics = @import("../observability/metrics_counters.zig");

const log = logging.scoped(.runner_retention_sweeper);

/// Terminal rows whose retention clock passed this are eligible. Lifetime
/// counters make the pruned history redundant for the operator surface; 30 days
/// keeps a month of row-level forensics past settlement.
const RETENTION_WINDOW_MS: i64 = 30 * std.time.ms_per_day;

comptime {
    // A lease's total wall clock is hard-capped: renewal clamps to
    // `created_at + MAX_RUNTIME_MS` and is refused past it, after which reclaim
    // flips the row to expired. So a live lease can never reach the age cutoff,
    // and neither can any event belonging to one — which is what lets the event
    // sweep below key on age alone with no lease-liveness join. Grow that
    // ceiling past the window and the sweep would start deleting a running
    // lease's records; fail the build instead of shipping that silently.
    if (constants.MAX_RUNTIME_MS >= RETENTION_WINDOW_MS)
        @compileError("RETENTION_WINDOW_MS must exceed MAX_RUNTIME_MS — an age-keyed sweep would reach live work");
}

/// The tag names the event sweep is allowed to delete, derived from the one
/// contract-side list so a new per-work tag cannot be added without landing on
/// one side of the retention decision.
const PER_LEASE_EVENT_TAGS = blk: {
    var tags: [protocol.PER_LEASE_EVENT_TYPES.len][]const u8 = undefined;
    for (protocol.PER_LEASE_EVENT_TYPES, 0..) |event_type, i| tags[i] = @tagName(event_type);
    break :blk tags;
};
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

/// Keyed on `id` (UNIQUE) so the outer DELETE stays a PK-shaped semi-join; the
/// inner SELECT rides slot 046's `(status, updated_at)` index over the
/// terminal/aged subset only. `updated_at` — not `created_at` — is the
/// retention clock: settle and reclaim both stamp it, so the window counts from
/// the settlement the API documents.
///
/// `FOR UPDATE SKIP LOCKED` because every replica runs its own sweeper: without
/// it a second replica blocks on the first's row locks and then deletes nothing,
/// paying full search cost for zero work. With it, concurrent sweepers take
/// disjoint batches.
pub const DELETE_TERMINAL_LEASES_BATCH =
    \\DELETE FROM fleet.runner_leases
    \\WHERE id IN (
    \\  SELECT id FROM fleet.runner_leases
    \\  WHERE status = ANY($1::text[]) AND updated_at < $2
    \\  LIMIT $3
    \\  FOR UPDATE SKIP LOCKED
    \\)
;

/// Per-work event rows only (`$1`), aged past the cutoff (`$2`). The lifecycle
/// tags are never eligible — see the module note. Rides slot 046's
/// `(event_type, occurred_at)` index; same SKIP LOCKED reasoning as above.
pub const DELETE_AGED_RUNNER_EVENTS_BATCH =
    \\DELETE FROM fleet.runner_events
    \\WHERE id IN (
    \\  SELECT id FROM fleet.runner_events
    \\  WHERE event_type = ANY($1::text[]) AND occurred_at < $2
    \\  LIMIT $3
    \\  FOR UPDATE SKIP LOCKED
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
        const deleted = (try conn.exec(DELETE_AGED_RUNNER_EVENTS_BATCH, .{ &PER_LEASE_EVENT_TAGS, cutoff, DELETE_BATCH_LIMIT })) orelse 0;
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
