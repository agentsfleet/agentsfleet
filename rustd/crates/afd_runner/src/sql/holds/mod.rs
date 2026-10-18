//! `runner.fleet_slots.hold_expires_at` as a runner's beat reconciles it.
//!
//! Both statements are scoped to the beating runner: a hold is cleared only on
//! a slot whose `last_runner_id` is that runner, and a fleet counts as still
//! held only where it is.

/// Clears the hold on every slot `$1` is the last runner of but did not list,
/// unless the slot was written within the last beat interval and the list is
/// not final.
///
/// A runner lists every fleet it holds a sandbox for on each beat, so a fleet
/// missing from the list is held nowhere, and the slot stops steering that
/// fleet's next event to it. The runner lists its holds before its beat goes
/// out, so a report that commits a hold in between reaches a beat that leaves
/// it out: a slot written within `$3` is left to the next beat, which clears
/// it if the list still leaves it out. A closing runner's list is final, `$4`:
/// it has ended every hold and parks no more, so nothing it reported can still
/// stand. `$1` runner, `$2` listed fleet ids, `$3` the heartbeat interval, `$4`
/// whether the list is final.
pub const CLEAR_DROPPED_HOLDS: &str = include_str!("clear_dropped_holds.sql");

/// The listed fleets `$1` should let go of: every one that is not both active
/// and last leased by `$1`.
///
/// A fleet another runner has leased since, or one halted or deleted, gives
/// the same answer as one this runner never held, so the reply says nothing
/// about fleets outside the runner's own slots. `$1` runner, `$2` listed fleet
/// ids, `$3` the active fleet status.
pub const SELECT_HOLDS_TO_RELEASE: &str = include_str!("select_holds_to_release.sql");
