//! Guard reads acquire wallet, current subject, draft and attempt in that order.

/// Current chat work, excluding accounting retained below the Stop watermark.
///
/// `$1` draft, `$2` settled status, `$3` Stop watermark; caller holds the draft lock.
pub(super) const PENDING: &str = include_str!("pending.sql");

/// The runner's attempt for a lease, unlocked.
///
/// `$1` lease, `$2` runner.
pub(super) const FIND: &str = include_str!("find.sql");

/// [`FIND`], locked.
pub(super) const LOCK: &str = include_str!("lock.sql");

/// An attempt by its own id, locked, for the maintenance sweep.
///
/// `$1` attempt.
pub(crate) const LOCK_BY_ID: &str = include_str!("lock_by_id.sql");

/// Whether the attempt's message is still the one running.
///
/// `$1` draft, `$2` sequence, `$3` the running status.
pub(super) const MESSAGE: &str = include_str!("message.sql");

/// Whether a saved cutoff, a deletion or a lifecycle change fenced the message.
///
/// `$1` draft, `$2` sequence, `$3` the editing status.
pub(super) const CANCELLED: &str = include_str!("cancelled.sql");

/// Whether a dispatch of the attempt still has no known outcome.
///
/// `$1` attempt, `$2` and `$3` the unresolved dispatch statuses.
pub(super) const UNRESOLVED: &str = include_str!("unresolved.sql");

/// Keeps the daemon's first observed refusal on the attempt.
///
/// `$1` attempt, `$2` reason.
pub(super) const RECORD_FAILURE: &str = include_str!("record_failure.sql");

/// Extends a live lease from the clock read after its locks, never past the
/// attempt's deadline, and returns the stored expiry.
///
/// `$1` attempt, `$2` the locked clock reading, `$3` the lease lifetime,
/// `$4` the verified runtime in milliseconds.
pub(super) const EXTEND: &str = include_str!("extend.sql");
