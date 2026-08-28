//! The one read the activity verb makes.
//!
//! Text is byte-identical to the inline statement in `fleet/service_activity.zig`.

/// The fleet and event one lease's frames belong to, scoped to its runner.
///
/// Two columns and no `status`, which is the whole difference between this and
/// [`super::report::SELECT_LEASE_FOR_REPORT`]: a report must know the lease is
/// live, and a cosmetic frame must not care. The `runner_id` predicate is still
/// the ownership check — without it a runner could publish onto any fleet's
/// live tail by naming a lease id it does not hold.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_TARGET: &str = "\
SELECT fleet_id::text, event_id
FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid";
