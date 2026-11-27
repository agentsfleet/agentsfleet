//! `thread.threads.park_expires_at` as a runner's beat reconciles it.
//!
//! Both statements are scoped to the beating runner: a park is cleared only on
//! a thread whose `last_runner_id` is that runner, and a fleet counts as still
//! parked only where it is.

/// Clears the park on every thread `$1` is the last runner of but did not list,
/// unless the thread was written within the last beat interval and the list is
/// not final.
///
/// A runner lists every fleet it parks a sandbox for on each beat, so a fleet
/// missing from the list is parked nowhere, and the thread stops steering that
/// fleet's next event to it. The runner lists its parks before its beat goes
/// out, so a report that commits a park in between reaches a beat that leaves
/// it out: a thread written within `$3` is left to the next beat, which clears
/// it if the list still leaves it out. A closing runner's list is final, `$4`:
/// it has ended every park and parks no more, so nothing it reported can still
/// stand. `$1` runner, `$2` listed fleet ids, `$3` the heartbeat interval, `$4`
/// whether the list is final.
pub const CLEAR_DROPPED_PARKS: &str = include_str!("clear_dropped_parks.sql");

/// The listed fleets `$1` should let go of: every one that is not both active
/// and last run by `$1`.
///
/// A fleet another runner has run since, or one halted or deleted, gives
/// the same answer as one this runner never parked, so the reply says nothing
/// about fleets outside the runner's own threads. `$1` runner, `$2` listed fleet
/// ids, `$3` the active fleet status.
pub const SELECT_PARKS_TO_RELEASE: &str = include_str!("select_parks_to_release.sql");
