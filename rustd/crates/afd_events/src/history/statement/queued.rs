//! The waiting read's texts: a fleet's steers on the admission ledger that no
//! runner has yet. Split from [`super`] at its length cap; they read
//! `core.fleet_admissions` and share none of its macros.

/// The waiting read's columns and scope: a fleet's steers on its queue that
/// no runner has yet.
macro_rules! queued_scope {
    () => {
        "\
SELECT fleet_id::text, workspace_id::text, actor, event_type, request_json, created_at, seq \
FROM core.fleet_admissions \
WHERE fleet_id = $2::uuid AND workspace_id = $1::uuid AND producer = $3 \
  AND receipt IS NOT NULL AND delivered_at IS NULL "
    };
}

/// The waiting read's order and cut, newest first.
macro_rules! queued_newest_first {
    ($limit:literal) => {
        concat!("ORDER BY created_at DESC, seq DESC LIMIT $", $limit)
    };
}

/// The fleet's waiting steers, newest first: `$1` workspace, `$2` fleet, `$3`
/// producer, `$4` limit.
///
/// Both partial indexes on `delivered_at IS NULL` (`schema/910`,
/// `schema/914`) key on `(fleet_id, created_at, seq)` and hold only in-flight
/// work, so whichever the planner takes, the fleet is an index condition, the
/// order comes off the index, and the cost follows what waits, never the
/// ledger's history. The logical id is spelled in Rust
/// (`afd_admission::logical_id`), so the two integers come back as they are.
pub(in crate::history) const SELECT_THREAD_QUEUED: &str =
    concat!(queued_scope!(), queued_newest_first!(4));

/// The same, older than a resumed page's cursor: `$4` `created_at`, `$5`
/// `event_id`, `$6` limit.
///
/// The cursor is a history key, `(created_at, event_id)`, and a waiting row's
/// event id is the logical id `{created_at}-{seq}`; spelling it here compares
/// the two on the one key the merged page is sorted and cut on.
pub(in crate::history) const SELECT_THREAD_QUEUED_AFTER: &str = concat!(
    queued_scope!(),
    "AND (created_at, created_at::text || '-' || seq::text) < ($4, $5) ",
    queued_newest_first!(6)
);

/// The waiting-messages texts, for the suite that asks Postgres how it plans
/// them.
#[cfg(feature = "test-util")]
pub const QUEUED_READ_TEXTS: [(&str, &str); 2] = [
    ("waiting", SELECT_THREAD_QUEUED),
    ("waiting after", SELECT_THREAD_QUEUED_AFTER),
];
