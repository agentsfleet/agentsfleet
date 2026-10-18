//! The statements the tool-call record verb and settlement run.
//!
//! Each one names `fleet.tool_calls` (`schema/1710`).

/// The lease a record post names, if this runner holds it live, locked for
/// the length of the post's transaction: [`super::standing::SELECT_STANDING`]
/// with the row lock.
///
/// Answers the lease's own fencing token beside the fleet's live sequence, so
/// the caller can refuse a holder a reclaim has superseded. `LEFT JOIN`, as the
/// memory fence reads it: a lease whose fleet has no slot row is fenced by its
/// own token.
///
/// The lock is what makes the check hold until the write commits. A settle
/// (`CLAIM_AND_SETTLE` takes `FOR UPDATE OF l`) and a reclaim both change this
/// row, so neither can supersede the lease between this check and the upsert —
/// and a stale holder cannot write records after the settle that would have
/// deleted them. It also serialises two posts of one lease, so they cannot both
/// pass the budget. The lock is on the lease rather than the event row: a
/// settle takes the lease lock before the tenant's wallet, so it never waits on
/// a post while holding the wallet.
///
/// `$1` lease, `$2` runner, `$3` the active status.
pub const SELECT_LIVE_LEASE: &str =
    concat!(include_str!("live_lease.sql"), "FOR NO KEY UPDATE OF l");

/// The record bytes one lease of an event keeps.
///
/// `$1` fleet, `$2` event, `$3` fence.
pub const SELECT_KEPT_BYTES: &str = include_str!("select_kept_bytes.sql");

/// What each call this post names already spends, so a replacement is charged
/// only the difference.
///
/// `$1` fleet, `$2` event, `$3` fence, `$4` the call numbers being posted.
pub const SELECT_REPLACED_BYTES: &str = include_str!("select_replaced_bytes.sql");

/// Writes a post's records in one statement, replacing any posted before.
///
/// The unique key is the upsert's arbiter, so a retried post rewrites the
/// rows it already wrote and adds none. `id` and `created_at` are the first
/// write's.
///
/// `$1` workspace, `$2` fleet, `$3` event, `$4` fence, then one array per
/// column: `$5` ids, `$6` sequences, `$7` arguments as JSON text, `$8`
/// truncated arguments, `$9` outputs, `$10` line counts, `$11` truncated,
/// `$12` byte counts. Both stamps are `now()`.
pub const UPSERT_RECORDS: &str = include_str!("upsert_tool_calls.sql");

/// Deletes the records every other lease of an event kept.
///
/// Run in two places, each holding a lease that is the fleet's current one.
/// The settling transaction runs it, because a reclaimed lease re-ran the
/// event and numbered its calls from 1 again, so only the settling fence's
/// records describe the run whose answer stands. A post runs it too, so an
/// event reclaimed many times before it settles keeps one lease's records
/// rather than a budget's worth per reclaim.
///
/// `$1` fleet, `$2` event, `$3` the current fence.
pub const DELETE_OTHER_FENCES: &str = include_str!("delete_other_fences.sql");
