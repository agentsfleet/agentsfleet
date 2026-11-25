//! Run bookkeeping on `thread.threads` and `thread.runs`, keyed by a thread's
//! or a run's id. A thread's id is its fleet's, and a run reaches its thread
//! through `fleet_id`.
//!
//! # Where the fleet keeps a fused statement instead
//!
//! The fleet's assignment, issue, renewals and report each run ONE statement that
//! does this bookkeeping beside the fleet's own rows
//! (`../run/assign_turn_head.sql`, `../run/insert_run_with_turn.sql`,
//! `../renew_batch.sql`, `../meter_run.sql`, `../commit_report.sql`), so a
//! fleet chat pays one round trip for each.

/// Assigns a fleet's thread, bumping the monotonic fence.
///
/// `$1` fleet, `$2` runner, `$3` the run lifetime, `$4` how long a silent
/// runner stays live, `$5` the runner status that may run.
///
/// Answers no row when a live runner still holds the thread: that absence is
/// the taken verdict, not an error. While `hold_expires_at` is in the future
/// and its holder can still run, only the holder wins, and the holder's own
/// assignment keeps the hold. `prior` reads the thread before the statement changes
/// it, so the caller can tell an assignment on a held thread from one on an unheld
/// thread. One `clock_timestamp()` reading decides every deadline.
///
/// The fleet's production assignment runs this upsert as the `won_thread` step of
/// `assign_turn_head.sql`; the test seam `Runs::assign` runs it alone.
pub const ASSIGN_THREAD: &str = include_str!("assign_thread.sql");

/// Releases a thread, fencing-guarded so only the current holder can free it,
/// and records whether that holder keeps the owner's sandbox.
///
/// `$1` thread, `$2` fence, `$3` when the runner asked its hold to lapse (NULL
/// for none), `$4` the longest hold. The hold written is the ask capped at one
/// `$4` window from `clock_timestamp()`, and none when the ask is already past.
/// The freed deadline is written one millisecond behind the clock: a
/// `TIMESTAMPTZ(3)` column rounds, and a deadline rounded up past the instant
/// an assignment reads a moment later would leave a released thread looking held.
///
/// The guard is load-bearing: a holder superseded by a reassignment would otherwise
/// free the current holder's thread and hand one owner's work to two runners.
/// Idempotent: a no-op once the token has moved on.
pub const RELEASE_THREAD: &str = include_str!("release_thread.sql");

/// [`RELEASE_THREAD`] for an assignment that held nothing. `$1` thread, `$2` fence.
///
/// It drops the sticky hint too, or an owner whose pass keeps stopping sorts
/// first for that runner on every poll, starving the rest. A thread still held
/// keeps its hint: the hint names the holder, and the hold steers the owner's
/// next work there.
pub const RELEASE_RUNLESS_THREAD: &str = include_str!("release_runless_thread.sql");

/// Locks a run's thread and then the run, and answers whether its Kill mark
/// is set. `$1` run, `$2` runner.
///
/// Thread then run is every renewal's lock order, held until the renewal
/// commits or is refused; a run that is not this runner's answers no row.
pub const LOCK_CANCELLED: &str = include_str!("lock_cancelled.sql");

/// The hold a settled run's report recorded, while it is still live.
///
/// `$1` run, `$2` runner, `$3` the reported status. A receipt read never
/// renews or recreates a hold after a later issue consumed it.
pub const REPORT_HOLD: &str = include_str!("report_hold.sql");
