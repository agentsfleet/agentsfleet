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
//!  * Events age from `created_at`, and only the per-work tags are eligible.
//!    The lifecycle tags are the Activity feed's entire content and are kept:
//!    pruning them by age blanked the feed for every runner enrolled before the
//!    window, which is exactly the operator surface this sweeper exists to keep
//!    fast.
//!  * Live work is untouchable by construction: the lease predicate admits
//!    terminal statuses only, and the comptime check below proves no live
//!    lease can reach the age cutoff, so no live lease's events can either.
//!
//! The sweeper is also the lease status column's only clock-driven writer, and
//! that is why it exists here rather than in a sweeper of its own. Three writers
//! can move a lease out of `active`: the runner's report, the fleet's NEXT claim
//! (`reclaim.reclaimPriorActive`), and the fleet's deletion. A run whose runner
//! died, whose event was then settled terminally by another path (so nothing
//! redelivers), on a fleet its owner never messages again, meets none of them —
//! the row is `active` forever. Harmless until this sweeper: the event pass
//! prunes such a lease's per-work records by age while the lease pass spares the
//! row itself, leaving an eternal "running" lease with its own history erased,
//! and the comptime proof's premise ("reclaim flips it") is false for exactly
//! this class. `expireAbandoned` is the missing fourth writer. It keys on the
//! same 30-day cutoff — sixty times the `MAX_RUNTIME_MS` ceiling a live lease
//! cannot outlive — so it can only ever reach rows whose grip provably ended,
//! and the `expired` tally rides the flip exactly as reclaim's does. The flip
//! stamps `updated_at`, so a reaped lease then keeps the same 30-day readable
//! window every settled lease gets: bounded at 60 days total, never unbounded.

const std = @import("std");
const constants = @import("common");
const clock = constants.clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const db = @import("../db/pool.zig");
const protocol = @import("contract").protocol;
const metrics = @import("../observability/metrics_counters.zig");

const log = logging.scoped(.runner_retention_sweeper);

/// Terminal rows whose retention clock passed this are eligible. Lifetime
/// counters make the pruned history redundant for the operator surface; 30 days
/// keeps a month of row-level forensics past settlement.
const RETENTION_WINDOW_MS: i64 = 30 * std.time.ms_per_day;

comptime {
    // A lease's total wall clock is hard-capped: renewal clamps to
    // `created_at + MAX_RUNTIME_MS`, is refused past it, and every renewal
    // stamps `updated_at` on the way through. So a lease anything still holds
    // is at most `MAX_RUNTIME_MS` stale, and neither it nor any event belonging
    // to it can reach the age cutoff — which is what lets the event sweep key on
    // age alone with no lease-liveness join, and what makes `expireAbandoned`
    // safe to flip a row it finds past the cutoff. (The premise is the renewal
    // ceiling, NOT that reclaim eventually runs: for an abandoned fleet it never
    // does — see the module note.) Grow that ceiling past the window and the
    // sweep would start reaching running work; fail the build instead of
    // shipping that silently.
    if (constants.MAX_RUNTIME_MS >= RETENTION_WINDOW_MS)
        @compileError("RETENTION_WINDOW_MS must exceed MAX_RUNTIME_MS — an age-keyed sweep would reach live work");
}

/// The tag names the event sweep is allowed to delete, derived from the one
/// contract-side list so a new per-work tag cannot be added without landing on
/// one side of the retention decision.
pub const PER_LEASE_EVENT_TAGS = blk: {
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
/// Re-arm delay after a cycle that filled every batch. A saturated cycle means
/// the backlog outran one cycle's ceiling, so the hour-long idle that follows a
/// drained sweep would cap throughput at `MAX_BATCHES_PER_CYCLE ×
/// DELETE_BATCH_LIMIT` rows per table per replica-hour — under a sustained lease
/// rate above that, the backlog grows while every cycle reports success. Only
/// the idle gap shrinks: `DELETE_BATCH_LIMIT` still bounds lock time and
/// write-ahead log per statement, which is what makes the sweep safe to run.
const SWEEP_SATURATED_INTERVAL_NS: u64 = std.time.ns_per_min;
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
/// `(event_type, created_at)` index; same SKIP LOCKED reasoning as above.
pub const DELETE_AGED_RUNNER_EVENTS_BATCH =
    \\DELETE FROM fleet.runner_events
    \\WHERE id IN (
    \\  SELECT id FROM fleet.runner_events
    \\  WHERE event_type = ANY($1::text[]) AND created_at < $2
    \\  LIMIT $3
    \\  FOR UPDATE SKIP LOCKED
    \\)
;

/// Flip abandoned `active` leases (`$1`) whose last write predates the cutoff
/// (`$2`) to `expired` (`$3`), stamping `updated_at` (`$5`) so the reaped row
/// starts the same readable window a normally-settled lease gets.
///
/// The `expired` tally rides the same statement as the flip, mirroring
/// `reclaim.reclaimPriorActive` — the counter counts transitions, and this is
/// one, so it cannot drift from the rows it describes. Grouped by runner
/// because one batch can carry several of a runner's leases; `EXCLUDED.expired`
/// therefore adds the batch's count for that runner, not a bare 1.
///
/// Same `FOR UPDATE SKIP LOCKED` reasoning as the deletes below, and the same
/// slot-046 `(status, updated_at)` index serves the search.
///
/// Unlike the two sweeps below, this one is NOT pinned to a specific index, and
/// the difference is about what each searches rather than about care taken. The
/// deletes below select `reported`/`expired` — the bulk of a mature table — so
/// an index that cannot bound their age predicate walks essentially everything,
/// which is why slot 046 exists and why their plans are pinned. `active` is the
/// opposite: it is live work plus the rare stranded row, a small set at any
/// instant, so whichever index the planner picks it scans few entries and the
/// `LIMIT` short-circuits the moment a backlog does exist. Measured, both
/// choices were observed on the same data. What IS pinned is the floor: the
/// search must not fall back to a sequential scan of the table.
///
/// The status still binds as an array, matching its siblings' shape so all
/// three sweep statements read the same way — not for any plan benefit, which
/// was measured and found absent.
pub const EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH =
    \\WITH doomed AS (
    \\  SELECT id, runner_id FROM fleet.runner_leases
    \\  WHERE status = ANY($1::text[]) AND updated_at < $2
    \\  LIMIT $4
    \\  FOR UPDATE SKIP LOCKED
    \\), tally AS (
    \\  INSERT INTO fleet.runner_lifetime_counters
    \\    (runner_id, expired, created_at, updated_at)
    \\  SELECT d.runner_id, COUNT(*)::bigint, $5, $5
    \\  FROM doomed d GROUP BY d.runner_id
    \\  ON CONFLICT (runner_id) DO UPDATE
    \\     SET expired = fleet.runner_lifetime_counters.expired + EXCLUDED.expired,
    \\         updated_at = EXCLUDED.updated_at
    \\)
    \\UPDATE fleet.runner_leases AS l
    \\SET status = $3, updated_at = $5
    \\FROM doomed d
    \\WHERE l.id = d.id
;

pub const SweepTotals = struct {
    leases_deleted: i64 = 0,
    events_deleted: i64 = 0,
    /// Abandoned `active` rows this cycle flipped to `expired`.
    leases_expired: i64 = 0,
    /// True when any pass filled every batch it was allowed — the backlog
    /// outran the cycle and the next one should follow promptly.
    saturated: bool = false,
};

/// Run until shutdown is signalled. Spawned by the serve lifecycle.
pub fn run(pool: *db.Pool, shutdown: *std.atomic.Value(bool)) void {
    log.debug(LOG_SWEEPER_STARTED, .{ .interval_ms = SWEEP_INTERVAL_NS / std.time.ns_per_ms, .window_ms = RETENTION_WINDOW_MS });
    while (!shutdown.load(.acquire)) { // safe because: pairs with serve_shutdown's background-stop release-store.
        // Totals live out here so a mid-cycle failure still reports the rows
        // its earlier passes already committed: those deletions are durable
        // whatever happened next, and dropping them would under-report the
        // series an operator reads to tell a working sweeper from a stuck one.
        var totals = SweepTotals{};
        const failed = if (sweepInto(pool, &totals)) |_| false else |err| blk: {
            metrics.incRetentionSweepFailure();
            log.warn(LOG_SWEEP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            break :blk true;
        };
        reportCycle(totals);
        const idle_ns = if (!failed and totals.saturated) SWEEP_SATURATED_INTERVAL_NS else SWEEP_INTERVAL_NS;
        sleepInterruptible(shutdown, idle_ns);
    }
    log.debug(LOG_SWEEPER_STOPPED, .{});
}

/// Publish a cycle's work. Silent when it moved nothing, so a quiet sweeper on
/// an idle control plane does not narrate an empty pass every hour.
fn reportCycle(totals: SweepTotals) void {
    const swept = totals.leases_deleted + totals.events_deleted;
    if (swept > 0) metrics.addRetentionSwept(@intCast(swept));
    if (swept == 0 and totals.leases_expired == 0) return;
    log.info(LOG_SWEEP_COMPLETED, .{
        .leases_deleted = totals.leases_deleted,
        .events_deleted = totals.events_deleted,
        .leases_expired = totals.leases_expired,
        .saturated = totals.saturated,
    });
}

/// Execute one bounded sweep cycle. Tests call this directly.
pub fn sweepOnce(pool: *db.Pool) !SweepTotals {
    var totals = SweepTotals{};
    try sweepInto(pool, &totals);
    return totals;
}

/// One cycle, accumulating into `totals` as each pass commits — so a caller
/// handling the error still learns what landed before it.
///
/// Order matters: abandoned rows are flipped first, but the flip stamps
/// `updated_at = now`, so they are deliberately NOT eligible for the lease
/// delete pass that follows. A reaped lease serves its readable window like any
/// other settled one and leaves on a later cycle.
fn sweepInto(pool: *db.Pool, totals: *SweepTotals) !void {
    const now_ms = clock.nowMillis();
    const cutoff = now_ms - RETENTION_WINDOW_MS;
    const terminal = [_][]const u8{
        protocol.RUNNER_LEASE_STATUS_REPORTED,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
    };
    const conn = try pool.acquire();
    defer pool.release(conn);

    const abandoned = [_][]const u8{protocol.RUNNER_LEASE_STATUS_ACTIVE};
    totals.saturated = try runBatched(conn, EXPIRE_ABANDONED_ACTIVE_LEASES_BATCH, .{
        &abandoned,
        cutoff,
        protocol.RUNNER_LEASE_STATUS_EXPIRED,
        DELETE_BATCH_LIMIT,
        now_ms,
    }, &totals.leases_expired) or totals.saturated;

    totals.saturated = try runBatched(conn, DELETE_TERMINAL_LEASES_BATCH, .{
        &terminal,
        cutoff,
        DELETE_BATCH_LIMIT,
    }, &totals.leases_deleted) or totals.saturated;

    totals.saturated = try runBatched(conn, DELETE_AGED_RUNNER_EVENTS_BATCH, .{
        &PER_LEASE_EVENT_TAGS,
        cutoff,
        DELETE_BATCH_LIMIT,
    }, &totals.events_deleted) or totals.saturated;
}

/// Repeat `sql` until a batch comes back short (the pass drained) or the
/// per-cycle ceiling is reached. Adds every batch's affected rows to `out`
/// before it can fail, so a failure mid-pass still leaves the committed count
/// visible. Returns true when the ceiling was hit with batches still full.
fn runBatched(conn: *pg.Conn, sql: []const u8, args: anytype, out: *i64) !bool {
    var batch: usize = 0;
    while (batch < MAX_BATCHES_PER_CYCLE) : (batch += 1) {
        const affected = (try conn.exec(sql, args)) orelse 0;
        out.* += affected;
        if (affected < DELETE_BATCH_LIMIT) return false;
    }
    return true;
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
