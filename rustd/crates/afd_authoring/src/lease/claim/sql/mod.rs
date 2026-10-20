//! Hints never authorize admission; the guarded transaction rereads every fact.

pub(super) const CONTEXT: &str = "admit queued authoring message";
pub(super) const SCAN_LIMIT: i64 = 64;

/// Drafts with queued work and no uncancelled attempt, oldest request first.
///
/// `$1` the editing draft status, `$2` the queued message status, `$3` the
/// settled attempt status, `$4` limit.
pub(super) const CANDIDATES: &str = include_str!("candidates.sql");

/// The draft's next queued message past its cutoff, with its pinned model.
///
/// `$1` draft, `$2` the queued status, `$3` the cutoff.
pub(super) const MESSAGE: &str = include_str!("message.sql");

/// `$1` draft, `$2` the admitted sequence, `$3` limit.
pub(super) const HISTORY: &str = include_str!("history.sql");

/// The admitted attempt; counters start at zero and stamps are the database's.
///
/// `$1` id, `$2` lease, `$3` runner, `$4` workspace, `$5` tenant, `$6` creator,
/// `$7` draft, `$8` sequence, `$9` fencing token, `$10` model token hash,
/// `$11` model snapshot, `$12` status, `$13` lease expiry, `$14` deadline,
/// `$15` reserved nanos.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// `$1` draft, `$2` sequence, `$3` the running status, `$4` the queued status.
pub(super) const RUNNING: &str = include_str!("running.sql");

/// Cancels a revoked creator's queued messages.
///
/// `$1` draft, `$2` workspace, `$3` tenant, `$4` creator, `$5` the cancelled
/// status, `$6` the queued status.
pub(super) const CANCEL_REVOKED: &str = include_str!("cancel_revoked.sql");
