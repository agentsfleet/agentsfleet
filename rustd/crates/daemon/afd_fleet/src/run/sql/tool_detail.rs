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
/// deleted them. It also serialises two posts of one run. The lock is on the
/// run rather than the event row: a
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

/// Which of the turns a post's records name the run serves: its own, or one
/// its conversation folded in at its fence.
///
/// `$1` fleet, `$2` run, `$3` fence, `$4` the turns named, `$5` the run's
/// own turn.
pub const SELECT_SERVED_TURNS: &str = include_str!("select_served_turns.sql");

/// Writes a post's records in one statement, replacing any posted before.
///
/// The unique key `(run_id, sequence)` is the upsert's arbiter, so a retried
/// post rewrites the rows it already wrote and adds none. `id` and
/// `created_at` are the first write's. A record naming a turn the run does
/// not serve joins no turn and is not written.
///
/// `$1` workspace, `$2` fleet, `$3` the run's own turn, `$4` fence, `$12`
/// run, then one array per column: `$5` ids, `$6` sequences, `$7` arguments
/// as JSON text, `$8` truncated arguments, `$9` outputs, `$10` line counts,
/// `$11` truncated, `$13` turns, `$14` kinds, `$15` tools, `$16` statuses,
/// `$17` changes as JSON text or NULL. Both stamps are `now()`.
pub const UPSERT_RECORDS: &str = include_str!("upsert_tool_calls.sql");

/// Deletes the records every other run of the run's turn kept.
///
/// A post runs it, holding a run that is the fleet's current one, so a turn
/// reassigned many times before it settles keeps one run's records rather
/// than a set per reassignment. The report statement deletes the
/// same records inline when it settles, because a reassigned run re-ran the
/// turn and numbered its calls from 1 again, so only the settling run's
/// records describe the run whose answer stands.
///
/// `$1` fleet, `$2` the run's own turn, `$3` the current run.
pub const DELETE_OTHER_FENCES: &str = include_str!("delete_other_fences.sql");
