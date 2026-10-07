//! `fleet.runner_affinity.held_until` as a runner's beat reconciles it.
//!
//! Both statements are scoped to the beating runner: a hold is cleared only on
//! a slot whose `last_runner_id` is that runner, and a fleet counts as still
//! held only where it is.

/// Clears the hold on every slot `$1` is the last runner of but did not list,
/// unless the slot was written within the last beat interval.
///
/// A runner lists every fleet it holds a sandbox for on each beat, so a fleet
/// missing from the list is held nowhere, and the slot stops steering that
/// fleet's next event to it. The runner lists its holds before its beat goes
/// out, so a report that commits a hold in between reaches a beat that leaves
/// it out: a slot written within `$4` is left to the next beat, which clears
/// it if the list still leaves it out. `$1` runner, `$2` listed fleet ids,
/// `$3` now, `$4` the heartbeat interval.
pub const CLEAR_DROPPED_HOLDS: &str = "\
UPDATE fleet.runner_affinity SET held_until = NULL, updated_at = $3
WHERE last_runner_id = $1::uuid
  AND held_until IS NOT NULL
  AND updated_at <= $3 - $4
  AND NOT (fleet_id = ANY(($2::text[])::uuid[]))";

/// The listed fleets `$1` should let go of: every one that is not both active
/// and last leased by `$1`.
///
/// A fleet another runner has leased since, or one halted or deleted, gives
/// the same answer as one this runner never held, so the reply says nothing
/// about fleets outside the runner's own slots. `$1` runner, `$2` listed fleet
/// ids, `$3` the active fleet status.
pub const SELECT_HOLDS_TO_RELEASE: &str = "\
SELECT listed::text
FROM unnest(($2::text[])::uuid[]) AS listed
WHERE NOT EXISTS (
  SELECT 1
  FROM fleet.runner_affinity a
  JOIN core.fleets z ON z.id = a.fleet_id
  WHERE a.fleet_id = listed AND a.last_runner_id = $1::uuid AND z.status = $3
)";
