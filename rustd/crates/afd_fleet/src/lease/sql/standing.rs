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
/// same read under a row lock, so each locking variant appends its clause to
/// the same text rather than spelling a second copy that could drift.
///
/// `$1` lease, `$2` runner, `$3` the active status. The deadline is read on
/// `clock_timestamp()`, which the locking uses below need: a row lock waited
/// on re-checks the predicate after it is granted.
pub const LIVE_LEASE: &str = include_str!("live_lease.sql");

/// [`LIVE_LEASE`], unlocked.
///
/// The schedules verbs act on other tables, and the messages verb writes only
/// `posted_message_count`, through [`COUNT_MESSAGE`], which re-checks the fence
/// itself, so neither holds a lock across the read.
pub const SELECT_STANDING: &str = LIVE_LEASE;

/// [`LIVE_LEASE`], holding the lease row shared until the caller's transaction
/// ends.
///
/// What a schedule write proves its lease with: a reclaim, a renew and a
/// settle each update this row, so none of them can land between the check
/// and the write it guards (`crate::lease::write_fence`).
pub const SELECT_STANDING_SHARED: &str = const_format::concatcp!(LIVE_LEASE, "FOR SHARE OF l");

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
/// `$1` lease, `$2` the cap, `$3` the presented token, `$4` the active status;
/// the deadline is read on `clock_timestamp()`, after the row lock.
pub const COUNT_MESSAGE: &str = include_str!("count_message.sql");
