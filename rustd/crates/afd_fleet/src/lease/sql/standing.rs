//! The statements a lease-addressed runner verb proves its lease with, and
//! the per-run message count.
//!
//! Each names `fleet.runner_leases` (`schema/610`, `schema/929`).

/// The lease a runner verb names, if this runner holds it live.
///
/// Answers the lease's fleet, workspace and event, its own fencing token, the
/// fleet's live sequence, and the event's actor, so the caller can refuse a
/// holder a reclaim has superseded and see whether a schedule woke the run. `LEFT JOIN`, as the memory fence reads it: a lease whose fleet
/// has no slot row is fenced by its own token.
///
/// A macro expanding to a literal, because the tool-call verb runs the same
/// read under a row lock and `concat!` takes literals only — one statement
/// text, two uses, no second spelling to drift.
///
/// `$1` lease, `$2` runner, `$3` the active status, `$4` now.
macro_rules! live_lease {
    () => {
        "\
SELECT l.fleet_id::text, l.workspace_id::text, l.event_id, l.fencing_token,
       COALESCE(a.fencing_seq, l.fencing_token) AS live_seq, l.actor
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4"
    };
}
pub(crate) use live_lease;

/// [`live_lease`], unlocked.
///
/// The schedules verbs act on other tables, and the messages verb writes only
/// `messages_posted`, through [`COUNT_MESSAGE`], which re-checks the fence
/// itself, so neither holds a lock across the read.
pub const SELECT_STANDING: &str = live_lease!();

/// [`live_lease`], holding the lease row shared until the caller's transaction
/// ends.
///
/// What a schedule write proves its lease with: a reclaim, a renew and a
/// settle each update this row, so none of them can land between the check
/// and the write it guards (`crate::lease::write_fence`).
pub const SELECT_STANDING_SHARED: &str = concat!(live_lease!(), "\nFOR SHARE OF l");

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
/// `$1` lease, `$2` the cap, `$3` the presented token, `$4` the active status,
/// `$5` now.
pub const COUNT_MESSAGE: &str = "\
UPDATE fleet.runner_leases l SET messages_posted = l.messages_posted + 1
WHERE l.id = $1::uuid AND l.messages_posted < $2
  AND l.fencing_token = $3 AND l.status = $4 AND l.lease_expires_at > $5
  AND l.fencing_token >= COALESCE(
        (SELECT a.fencing_seq FROM fleet.runner_affinity a WHERE a.fleet_id = l.fleet_id),
        l.fencing_token)
RETURNING l.messages_posted";
