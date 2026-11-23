//! The statements a run-addressed runner verb proves its run with.
//!
//! Each names `thread.runs` (`afd_db/schema/1810`).

/// The run a runner verb names, if this runner holds it live.
///
/// Answers the run's fleet, workspace and event, its own fencing token, the
/// fleet's live sequence, and the event's actor, so the caller can refuse a
/// holder a reassignment has superseded and see whether a schedule woke the run. `LEFT JOIN`, as the memory fence reads it: a run whose fleet
/// has no thread row is fenced by its own token.
///
/// One file, three uses: the tool-call verb and the schedule writes run the
/// same read under a row lock, so each locking variant puts its clause between
/// [`RUN_READ`] and [`RUN_JUDGED`] rather than spelling a second copy that
/// could drift.
///
/// `$1` run, `$2` runner, `$3` the active status.
///
/// # Why the deadline is judged outside the read
///
/// Postgres evaluates a locking statement's `WHERE` and select list before it
/// waits on a row lock, and re-checks them after the wait only when the lock's
/// holder WROTE the row. A holder that only locked it — a schedule write's
/// `FOR SHARE`, a record post's `FOR NO KEY UPDATE` — triggers no re-check, so a
/// deadline compared inside the locking read accepts a run that lapsed while
/// the read waited. The read is therefore a `MATERIALIZED` step that takes the
/// lock and answers `run_expires_at`; [`RUN_JUDGED`] reads
/// `clock_timestamp()` on the row it answered, once the lock is held.
pub(super) const RUN_READ: &str =
    const_format::concatcp!("WITH run AS MATERIALIZED (\n", include_str!("live_run.sql"));

/// Closes [`RUN_READ`] and keeps its row only while the run is live.
pub(super) const RUN_JUDGED: &str = "\n)\n\
SELECT fleet_id, workspace_id, turn_id, fence, live_seq, actor\n\
  FROM run\n\
 WHERE run_expires_at > clock_timestamp()\n";

/// [`RUN_READ`] and [`RUN_JUDGED`], unlocked.
///
/// The schedules verbs act on other tables and the messages verb writes
/// nothing to the run, so neither holds a lock across the read.
pub const SELECT_STANDING: &str = const_format::concatcp!(RUN_READ, RUN_JUDGED);

/// [`SELECT_STANDING`], holding the run row shared until the caller's
/// transaction ends.
///
/// What a schedule write proves its run with: a reassignment, a renew and a
/// settle each update this row, so none of them can land between the check
/// and the write it guards (`crate::run::write_fence`).
pub const SELECT_STANDING_SHARED: &str =
    const_format::concatcp!(RUN_READ, "FOR SHARE OF l", RUN_JUDGED);
