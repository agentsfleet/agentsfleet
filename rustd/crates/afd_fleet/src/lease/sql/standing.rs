//! The statements a lease-addressed runner verb proves its lease with, and
//! the per-run message count.
//!
//! Each names `fleet.runner_leases` (`schema/610`, `schema/929`).

/// The lease a runner verb names, if this runner holds it live.
///
/// Answers the lease's fleet, workspace and event, its own fencing token, and
/// the fleet's live sequence, so the caller can refuse a holder a reclaim has
/// superseded. `LEFT JOIN`, as the memory fence reads it: a lease whose fleet
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
       COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4"
    };
}
pub(crate) use live_lease;

/// [`live_lease`], unlocked: the schedules and messages verbs read the lease
/// and act on other tables, so nothing they write races a settle.
pub const SELECT_STANDING: &str = live_lease!();

/// Counts one interim message against its lease while the count is under the
/// cap, answering the new count; at the cap no row is touched or returned.
///
/// One guarded statement, so two concurrent posts cannot both take the last
/// slot.
///
/// `$1` lease, `$2` the cap.
pub const COUNT_MESSAGE: &str = "\
UPDATE fleet.runner_leases SET messages_posted = messages_posted + 1
WHERE id = $1::uuid AND messages_posted < $2
RETURNING messages_posted";
