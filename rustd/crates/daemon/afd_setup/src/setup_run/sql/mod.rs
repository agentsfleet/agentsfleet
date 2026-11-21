//! The draft kind's own statements on the common run path. The run
//! bookkeeping itself (thread, run, renewal, settlement) is
//! `afd_fleet::run::sql::shared`'s; these read and write only the draft's
//! rows beside it, in the same transactions.

/// Names the setup-run statements in a query failure.
pub(super) const CONTEXT: &str = "draft run setup turn";

/// How many drafts one assignment pass walks.
pub(super) const SCAN_LIMIT: i64 = 64;

/// Drafts whose head message a draft run may take.
///
/// `$1` editing, `$2` queued, `$3` running, `$4` limit, `$5` runner.
pub(super) const CANDIDATES: &str = include_str!("candidates.sql");

/// The draft's head message under its lock. `$1` draft, `$2` queued, `$3`
/// running.
pub(super) const HEAD: &str = include_str!("head.sql");

/// Every earlier answered turn, newest first. `$1` draft, `$2` sequence,
/// `$3` completed.
pub(super) const HISTORY: &str = include_str!("history.sql");

/// The message a draft run takes, running. `$1` draft, `$2` sequence, `$3`
/// running, `$4` queued.
pub(super) const RUNNING: &str = include_str!("running.sql");

/// The turn a draft run's report finishes, with any refused proposal's
/// detail. `$1` draft, `$2` sequence, `$3` status, `$4` answer, `$5`
/// candidate, `$6` failure reason, `$7` failure detail, `$8` running.
pub(super) const FINISH: &str = include_str!("finish.sql");

/// The draft a report finishes, locked, and its message. `$1` draft, `$2`
/// message.
pub(super) const LOCK_REPORT: &str = include_str!("lock_report.sql");

/// Whether a draft run may still publish its live answer. `$1` run, `$2`
/// runner, `$3` active, `$4` running.
pub(super) const PREVIEW: &str = include_str!("preview.sql");

/// Deletes a deleted draft's active runs. `$1` draft, `$2` active.
pub(crate) const DELETE_RUNS: &str = include_str!("delete_runs.sql");

/// The owner of the thread a run's context save names, its fleet or its
/// draft, if the run is this runner's. `$1` run, `$2` runner.
pub(super) const THREAD_OF_RUN: &str = include_str!("thread_of_run.sql");

/// The creator's row, shared, refusing rather than waiting when another
/// transaction holds it. `$1` user.
pub(super) const HOLD_USER: &str = include_str!("hold_user.sql");

/// The draft's row, refusing rather than waiting when another transaction
/// holds it. `$1` draft.
pub(super) const HOLD_DRAFT: &str = include_str!("hold_draft.sql");

/// Postgres's `lock_not_available`, which a `NOWAIT` lock raises when the row
/// is held.
pub(super) const LOCK_NOT_AVAILABLE: &str = "55P03";

/// Cancels a revoked creator's queued messages. `$1` draft, `$2` workspace,
/// `$3` tenant, `$4` creator, `$5` cancelled, `$6` queued.
pub(super) const CANCEL_REVOKED: &str = include_str!("cancel_revoked.sql");

/// Whether the draft's thread holds a live run. `$1` draft.
pub(super) const LIVE: &str = include_str!("live.sql");
