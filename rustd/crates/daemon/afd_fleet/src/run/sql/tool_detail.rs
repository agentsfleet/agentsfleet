//! The statements the tool-call record verb and settlement run.
//!
//! Each one names `thread.items` (`afd_db/schema/1710`).

/// The run a record post names, if this runner holds it live, locked for
/// the length of the post's transaction: [`super::standing::SELECT_STANDING`]
/// with the row lock.
///
/// Answers the run's own fencing token beside the fleet's live sequence, so
/// the caller can refuse a holder a reassignment has superseded. `LEFT JOIN`, as the
/// memory fence reads it: a run whose fleet has no thread row is fenced by its
/// own token.
///
/// The lock is what makes the check hold until the write commits. A settle
/// (the report statement locks the run) and a reassignment both change this
/// row, so neither can supersede the run between this check and the upsert —
/// and a stale holder cannot write records after the settle that would have
/// deleted them. It also serialises two posts of one run, so they cannot both
/// pass the budget. The lock is on the run rather than the event row: a
/// settle takes the run lock before the tenant's wallet, so it never waits on
/// a post while holding the wallet.
///
/// `$1` run, `$2` runner, `$3` the active status. The deadline is judged
/// after the lock wait, for the reason [`super::standing::RUN_READ`] gives.
pub const SELECT_LIVE_RUN: &str = const_format::concatcp!(
    super::standing::RUN_READ,
    "FOR NO KEY UPDATE OF l",
    super::standing::RUN_JUDGED
);

/// The record bytes one run of an event keeps.
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

/// Deletes the records every other run of an event kept.
///
/// A post runs it, holding a run that is the fleet's current one, so an
/// event reassigned many times before it settles keeps one run's records
/// rather than a budget's worth per reassignment. The report statement deletes the
/// same records inline when it settles, because a reassigned run re-ran the
/// event and numbered its calls from 1 again, so only the settling fence's
/// records describe the run whose answer stands.
///
/// `$1` fleet, `$2` event, `$3` the current fence.
pub const DELETE_OTHER_FENCES: &str = include_str!("delete_other_fences.sql");
