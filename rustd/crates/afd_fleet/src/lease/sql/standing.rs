//! The statements a lease-addressed runner verb proves its lease with, and
//! the per-run message count.
//!
//! Each names `runner.leases` (`schema/1810`).

/// The lease a runner verb names, if this runner holds it live.
///
/// Answers the lease's fleet, workspace and event, its own fencing token, the
/// fleet's live sequence, and the event's actor, so the caller can refuse a
/// holder a reclaim has superseded and see whether a schedule woke the run. `LEFT JOIN`, as the memory fence reads it: a lease whose fleet
/// has no slot row is fenced by its own token.
///
/// One file, three uses: the tool-call verb and the schedule writes run the
/// same read under a row lock, so each locking variant puts its clause between
/// [`LEASE_READ`] and [`LEASE_JUDGED`] rather than spelling a second copy that
/// could drift.
///
/// `$1` lease, `$2` runner, `$3` the active status.
///
/// # Why the deadline is judged outside the read
///
/// Postgres evaluates a locking statement's `WHERE` and select list before it
/// waits on a row lock, and re-checks them after the wait only when the lock's
/// holder WROTE the row. A holder that only locked it — a schedule write's
/// `FOR SHARE`, a record post's `FOR NO KEY UPDATE` — triggers no re-check, so a
/// deadline compared inside the locking read accepts a lease that lapsed while
/// the read waited. The read is therefore a `MATERIALIZED` step that takes the
/// lock and answers `lease_expires_at`; [`LEASE_JUDGED`] reads
/// `clock_timestamp()` on the row it answered, once the lock is held.
pub(super) const LEASE_READ: &str = const_format::concatcp!(
    "WITH lease AS MATERIALIZED (\n",
    include_str!("live_lease.sql")
);

/// Closes [`LEASE_READ`] and keeps its row only while the lease is live.
pub(super) const LEASE_JUDGED: &str = "\n)\n\
SELECT fleet_id, workspace_id, event_id, fence, live_seq, actor\n\
  FROM lease\n\
 WHERE lease_expires_at > clock_timestamp()\n";

/// [`LEASE_READ`] and [`LEASE_JUDGED`], unlocked.
///
/// The schedules verbs act on other tables, and the messages verb writes only
/// `posted_message_count`, through [`COUNT_MESSAGE`], which re-checks the fence
/// itself, so neither holds a lock across the read.
pub const SELECT_STANDING: &str = const_format::concatcp!(LEASE_READ, LEASE_JUDGED);

/// [`SELECT_STANDING`], holding the lease row shared until the caller's
/// transaction ends.
///
/// What a schedule write proves its lease with: a reclaim, a renew and a
/// settle each update this row, so none of them can land between the check
/// and the write it guards (`crate::lease::write_fence`).
pub const SELECT_STANDING_SHARED: &str =
    const_format::concatcp!(LEASE_READ, "FOR SHARE OF l", LEASE_JUDGED);

/// Counts one interim message against a lease that still holds its fleet.
///
/// Answers the new count while it is under the cap; a lease a reclaim
/// superseded since its standing was read, or one at the cap, touches no row
/// and returns none.
///
/// The fence is in the statement rather than only in the standing read before
/// it, so a holder superseded between the two cannot take a slot, and one
/// guarded statement means two concurrent posts cannot both take the last.
///
/// `$1` lease, `$2` the cap, `$3` the presented token, `$4` the active status.
/// The row is locked in a `MATERIALIZED` step and the deadline read on
/// `clock_timestamp()` in the next, for the reason [`LEASE_READ`] gives: an
/// `UPDATE` whose own `WHERE` read the clock would judge the lease before
/// waiting on a holder that only locked the row.
pub const COUNT_MESSAGE: &str = include_str!("count_message.sql");
