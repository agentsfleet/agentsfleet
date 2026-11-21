//! The statements the background sweepers run.
//!
//! They live in their own module rather than beside the runner store's for the
//! reason [`super`] gives: the split is by CALLER, and nothing on a request
//! path runs any of these.
//!
//! # Every one of them is bounded and idempotent
//!
//! A sweeper's pass can fail halfway and be re-run, so each statement either
//! changes nothing on a second pass (the offline event's unique stamp, the
//! drained transition's status predicate) or converges (the thread expiry's `>`
//! guard). That is what lets [`crate::sweep::run`] report a failure and carry
//! on rather than having to unwind one.
//!
//! # Every one of them reads the database's clock
//!
//! None takes a lock it then judges an expiry under, so each compares against
//! `statement_timestamp()` — the clock an index can bound, and the same instant
//! for every row one statement visits. Spans bind as intervals.

/// The runners one liveness pass considers.
///
/// Three disjunctions, and each is a different reason a runner needs looking
/// at: it has gone quiet, it is draining, or it is not active and still holds
/// runs. A runner that is none of these is healthy and is not fetched at all
/// — which is what keeps the pass proportional to the trouble rather than to
/// the fleet. `checked_at` is the instant the staleness test used, so the pass
/// judges each row against the clock that selected it.
///
/// `ORDER BY r.updated_at ASC, r.id ASC` with a LIMIT is the pass's bound: the
/// least recently touched runners are swept first, so no runner can be starved
/// by a busier neighbour, and `id` settles a same-millisecond tie so the order
/// is total.
///
/// `$1` the never-seen sentinel, `$2` the offline threshold, `$3` the active
/// status, `$4` the active run status, `$5` draining, `$6` the batch limit.
pub const SELECT_DUE_RUNNERS: &str = include_str!("select_due_runners.sql");

/// Record a freshly proved silent episode and automatic quarantine atomically.
pub const CORDON_SILENT_RUNNER: &str = include_str!("cordon_silent_runner.sql");

/// Finishes draining a runner once its last run is gone, and records the
/// transition in the same statement.
///
/// The `NOT EXISTS` guard is the safety property: a draining runner still
/// holding a run is still working, and marking it drained would tell an
/// operator it was safe to take the host away mid-run. The event is written in
/// the same statement as the status change, so history cannot disagree with
/// the row it describes.
///
/// `$1` runner, `$2` drained, `$3` draining, `$4` the active run status, `$5`
/// event id, `$6` event type, `$7` the from-status key, `$8` the to-status key.
pub const MARK_DRAINED_IF_IDLE: &str = include_str!("mark_drained_if_idle.sql");

/// Every fleet, counted by its stored status.
///
/// No `WHERE` and no `LIMIT`: the answer is one row per distinct status, so
/// the result is bounded by the status vocabulary and never by the table.
pub const COUNT_FLEETS_BY_STATUS: &str = include_str!("count_fleets_by_status.sql");

/// Unsettled turns and age of the oldest, from the queue index.
pub const COUNT_TURN_BACKLOG: &str = include_str!("count_turn_backlog.sql");
