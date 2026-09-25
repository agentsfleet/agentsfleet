//! The one read the activity verb makes.
//!
//! Text is byte-identical to the inline statement in `fleet/service_activity.zig`.

/// The fleet and event one lease's frames belong to, scoped to its runner.
///
/// The row is returned even when inactive, because a cosmetic frame may still
/// publish; the caller uses status and expiry only to decide whether timing is
/// eligible. The `runner_id` predicate is still
/// the ownership check — without it a runner could publish onto any fleet's
/// live tail by naming a lease id it does not hold.
///
/// `$1` lease, `$2` runner.
pub const SELECT_LEASE_TARGET: &str = "\
SELECT fleet_id::text, event_id, created_at, event_created_at, status, lease_expires_at
FROM fleet.runner_leases WHERE id = $1::uuid AND runner_id = $2::uuid";
