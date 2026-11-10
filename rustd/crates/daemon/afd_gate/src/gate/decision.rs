//! A human's answer, and the collapse of the durable row's `status` into one.
//!
//! The row's `status` is [`GateStatus`], shared with the operator plane that
//! writes it, and it is finer than an [`Answer`]. The collapse from one to the
//! other, [`answer`], lives here because it is this plane's.

use afd_api_wire::approval::GateStatus;

/// A decision, once a human has given one.
///
/// Two arms and not four: a timeout and an auto-kill both RESOLVE to a
/// refusal, and the distinction between them belongs to the durable row an
/// operator reads, not to the question "may this event run".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// A human said yes.
    Approved,
    /// A human said no, or the gate lapsed into one.
    Denied,
}

/// The answer the durable row's `status` resolves to, if it resolves to one.
///
/// The status is finer than [`Answer`]: "the reviewer said no", "nobody
/// answered in time" and "the daemon stopped it" are three incidents an
/// operator reads and a runbook branches on, and this is where they collapse
/// into one refusal. `None` is [`GateStatus::Pending`] and only that — which is
/// why the durable read can return an `Option` rather than needing a caller to
/// remember that pending is not terminal.
#[must_use]
pub(crate) const fn answer(status: GateStatus) -> Option<Answer> {
    match status {
        GateStatus::Pending => None,
        GateStatus::Approved => Some(Answer::Approved),
        GateStatus::Denied | GateStatus::TimedOut | GateStatus::AutoKilled => Some(Answer::Denied),
    }
}

#[cfg(test)]
mod tests {
    use af_core::spelling::from_spelling;
    use afd_api_wire::approval::GateStatus;

    use super::{Answer, answer};

    #[test]
    fn every_refusing_status_resolves_to_one_denial() {
        assert_eq!(answer(GateStatus::Approved), Some(Answer::Approved));
        for refusing in [
            GateStatus::Denied,
            GateStatus::TimedOut,
            GateStatus::AutoKilled,
        ] {
            assert_eq!(answer(refusing), Some(Answer::Denied), "{refusing:?}");
        }
        // And pending is the only status with no answer, which is what lets the
        // durable read hand back an `Option` instead of a terminality check.
        assert_eq!(answer(GateStatus::Pending), None);
    }

    #[test]
    fn a_status_reads_the_spellings_the_column_stores() {
        for (spelling, status) in [
            ("pending", GateStatus::Pending),
            ("approved", GateStatus::Approved),
            ("denied", GateStatus::Denied),
            ("timed_out", GateStatus::TimedOut),
            ("auto_killed", GateStatus::AutoKilled),
        ] {
            assert_eq!(
                from_spelling::<GateStatus>(spelling),
                Some(status),
                "{spelling}"
            );
        }
        // A word the column never held stays unresolved, which leaves the gate
        // pending rather than guessing in either direction.
        for unknown in ["cancelled", "TimedOut", "timedOut", ""] {
            assert_eq!(from_spelling::<GateStatus>(unknown), None, "{unknown}");
        }
    }
}
