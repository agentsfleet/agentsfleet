//! The waiting read's texts: a fleet's steers on the admission ledger that no
//! runner has yet. Split from [`super`] at its length cap; they read
//! `fleet.admissions` and share none of its column list.
//!
//! An admitted event is named by its admission row's id, so a waiting row's
//! `event_id` is that id as text and its `created_at` the admission's own —
//! the key the history row will carry once a runner leases it
//! (`crate::sql::INSERT_LEASED_FLEET_EVENT` stamps it from the same row). The
//! two reads therefore order and cut on one key, `(created_at, event_id)`.

/// The fleet's waiting steers, newest first: `$1` workspace, `$2` fleet, `$3`
/// producer, `$4` limit.
///
/// `idx_admissions_delivery_lookup` keys `(fleet_id, created_at, id)` over
/// `delivered_at IS NULL` and holds only in-flight work, so the fleet is an
/// index condition, the order comes off the index, and the cost follows what
/// waits, never the ledger's history.
pub(in crate::history) const SELECT_THREAD_QUEUED: &str = include_str!("select_thread_queued.sql");

/// The same, older than a resumed page's cursor: `$4` cursor instant, `$5`
/// cursor event id, `$6` limit.
///
/// The id is compared as TEXT, because the cursor's event id may name a
/// history row that is not an admission (a continuation, a schedule run) and a
/// `::uuid` cast of it would fail the read. Lowercase hex text orders the same
/// as the UUID's bytes, so the comparison and the `ORDER BY id` agree.
pub(in crate::history) const SELECT_THREAD_QUEUED_AFTER: &str =
    include_str!("select_thread_queued_after.sql");

/// The waiting-messages texts, for the suite that asks Postgres how it plans
/// them.
#[cfg(feature = "test-util")]
pub const QUEUED_READ_TEXTS: [(&str, &str); 2] = [
    ("waiting", SELECT_THREAD_QUEUED),
    ("waiting after", SELECT_THREAD_QUEUED_AFTER),
];
