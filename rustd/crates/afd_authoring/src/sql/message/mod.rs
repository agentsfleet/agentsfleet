//! Request sequence and its selected model are accepted in the draft transaction.
//!
//! A message's retry key lives in `authoring.replay_receipts` under the
//! message's own id, and its usage is the attempt's dispatch receipts, summed
//! once a report was acknowledged: neither is a message column.

/// `$1` id, `$2` draft, `$3` sequence, `$4` status, `$5` tenant model (NULL for
/// the platform default), `$6` user text.
pub(crate) const INSERT: &str = include_str!("insert.sql");

/// `$1` draft, `$2` the new last sequence.
pub(crate) const ADVANCE: &str = include_str!("advance.sql");

/// `$1` draft, `$2` cutoff, `$3` cancelled status, `$4` and `$5` the in-flight
/// statuses it moves.
pub(crate) const CANCEL: &str = include_str!("cancel.sql");

/// One page of user and assistant roles, with each message's retry key and
/// reported usage.
///
/// `$1` draft, `$2` cursor sequence, `$3` cursor role, `$4` and `$5` the
/// in-flight statuses an answerless assistant role is hidden for, `$6` limit,
/// `$7` the known usage word, `$8` and `$9` the unresolved dispatch statuses.
pub(crate) const PAGE: &str = include_str!("page.sql");

/// The newest messages' states for a stream poll, with reported usage.
///
/// `$1` draft, `$2` limit, `$3` the known usage word, `$4` and `$5` the
/// unresolved dispatch statuses.
pub(crate) const NOTICES: &str = include_str!("notices.sql");

/// The sequence a replayed message was accepted at.
///
/// `$1` message, `$2` draft.
pub(crate) const SEQUENCE: &str = include_str!("sequence.sql");

/// Fails one message that still holds the expected status, so only the first
/// writer finishes it.
///
/// `$1` draft, `$2` sequence, `$3` the failed status, `$4` reason, `$5` the
/// status it must still hold.
pub(crate) const FAIL: &str = include_str!("fail.sql");
