//! The draft kind's own statements on the common run path. The run
//! bookkeeping itself (slot, run, renewal, settlement) is
//! `afd_fleet::run::sql::shared`'s; these read and write only the draft's
//! rows beside it, in the same transactions.

/// Names the setup-run statements in a query failure.
pub(super) const CONTEXT: &str = "draft run setup turn";

/// How many drafts one claim pass walks.
pub(super) const SCAN_LIMIT: i64 = 64;

/// Drafts whose head message a draft run may take.
///
/// `$1` editing, `$2` queued, `$3` running, `$4` the finished attempt
/// statuses, `$5` limit, `$6` runner.
pub(super) const CANDIDATES: &str = include_str!("candidates.sql");

/// The draft's head message under its lock. `$1` draft, `$2` watermark, `$3`
/// queued, `$4` running.
pub(super) const HEAD: &str = include_str!("head.sql");

/// Earlier answered turns, newest first. `$1` draft, `$2` sequence, `$3`
/// completed, `$4` limit.
pub(super) const HISTORY: &str = include_str!("history.sql");

/// The message a draft run takes, running. `$1` draft, `$2` sequence, `$3`
/// running, `$4` queued.
pub(super) const RUNNING: &str = include_str!("running.sql");

/// The draft a report finishes, locked, and its message. `$1` draft, `$2`
/// message.
pub(super) const LOCK_REPORT: &str = include_str!("lock_report.sql");

/// Whether a draft run may still publish its live answer. `$1` run, `$2`
/// runner, `$3` active, `$4` running.
pub(super) const PREVIEW: &str = include_str!("preview.sql");

/// Kills the runs of messages a Stop finished. `$1` draft, `$2` watermark,
/// `$3` active.
pub(crate) const CANCEL_RUNS: &str = include_str!("cancel_runs.sql");

/// Deletes a deleted draft's active runs. `$1` draft, `$2` active.
pub(crate) const DELETE_RUNS: &str = include_str!("delete_runs.sql");
