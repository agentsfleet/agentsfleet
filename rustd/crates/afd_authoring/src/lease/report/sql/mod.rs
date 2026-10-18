//! Content-free acknowledgement and final money writes are protected by the attempt lock.

/// `$1` attempt, `$2` report.
pub(super) const RECEIPT: &str = include_str!("receipt.sql");

/// The answer a report saved, while its private retention lasts.
///
/// `$1` attempt, `$2` report.
pub(super) const ANSWER: &str = include_str!("answer.sql");

/// `$1` id, `$2` attempt, `$3` report, `$4` accounting digest, `$5` acknowledgement.
pub(super) const ACKNOWLEDGE: &str = include_str!("acknowledge.sql");

/// `$1` id, `$2` draft, `$3` attempt, `$4` report, `$5` answer digest,
/// `$6` its retention window.
pub(super) const ANSWER_RECEIPT: &str = include_str!("answer_receipt.sql");

/// Closes or holds the attempt; its lease never outlives the report.
///
/// `$1` attempt, `$2` status, `$3` whether usage is unknown, `$4` verified
/// runtime in milliseconds, `$5` the clock the report was measured at, `$6`
/// the settled outcome (NULL while usage is unknown).
pub(super) const ATTEMPT: &str = include_str!("attempt.sql");

/// The message's first terminal write: it matches only a running message, so
/// the one report that finishes it is the one that matches.
///
/// `$1` draft, `$2` sequence, `$3` status, `$4` answer, `$5` candidate,
/// `$6` failure reason, `$7` the running status.
pub(super) const MESSAGE: &str = include_str!("message.sql");

/// A late report rewriting a message expiry already failed for unknown usage;
/// the message finished then, so this write is not a second finish.
///
/// [`MESSAGE`]'s binds, then `$8` the unknown-usage reason.
pub(super) const LATE_MESSAGE: &str = include_str!("late_message.sql");

/// Replaces an unknown-usage failure once its usage became known.
///
/// `$1` draft, `$2` sequence, `$3` the resolved reason, `$4` the unknown-usage
/// reason.
pub(super) const RESOLVED_REASON: &str = include_str!("resolved_reason.sql");

/// The outcome a settled attempt recorded, which every later replay answers.
///
/// `$1` attempt.
pub(super) const SETTLED_OUTCOME: &str = include_str!("settled_outcome.sql");
