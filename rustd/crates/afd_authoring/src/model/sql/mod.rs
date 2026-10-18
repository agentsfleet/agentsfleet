//! Dispatch identity is durable before a physical request and survives private deletion.

/// A dispatch by its own id or by its position in the attempt.
///
/// `$1` attempt, `$2` dispatch, `$3` sequence.
pub(super) const PRIOR: &str = include_str!("prior.sql");

/// The attempt's first dispatch without a decoded result.
///
/// `$1` attempt, `$2` the completed status.
pub(super) const UNFINISHED: &str = include_str!("unfinished.sql");

/// The intent row, written before the physical request; a duplicate inserts
/// nothing. Its private result expires a retention window after the intent.
///
/// `$1` id, `$2` attempt, `$3` sequence, `$4` request digest, `$5` the pending
/// status, `$6` the retention window.
pub(super) const INSERT: &str = include_str!("insert.sql");

/// `$1` attempt.
pub(super) const ADVANCE: &str = include_str!("advance.sql");

/// `$1` dispatch, `$2` the unknown status.
pub(super) const UNKNOWN: &str = include_str!("unknown.sql");

/// `$1` attempt, `$2` the awaiting-accounting status.
pub(super) const UNCERTAIN: &str = include_str!("uncertain.sql");

/// `$1` attempt, `$2` runner.
pub(super) const LOCK_ATTEMPT: &str = include_str!("lock_attempt.sql");

/// Records a provider outcome on a dispatch that has none yet.
///
/// `$1` dispatch, `$2` attempt, `$3` status, `$4` private result, `$5` usage,
/// `$6` and `$7` the unresolved statuses it may move from.
pub(super) const COMPLETE: &str = include_str!("complete.sql");

/// Adds a dispatch's output to the attempt and recomputes whether any dispatch
/// is still unresolved.
///
/// `$1` attempt, `$2` output tokens, `$3` whether the outcome is unknown, `$4`
/// the awaiting-accounting status, `$5` and `$6` the unresolved statuses.
pub(super) const METER: &str = include_str!("meter.sql");
