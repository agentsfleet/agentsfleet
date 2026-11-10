//! A gate already raised for one event, and how a later poll resolves it.
//!
//! # No thread waits for a human
//!
//! Parking an event writes a durable `fleet.approval_gates` row naming the
//! action it is waiting on. Every later lease poll finds that row and
//! re-evaluates it. There is no blocking wait, no parked handler, and no timer
//! — the deadline is the row's `expires_at`, stamped by the database clock
//! when the gate was raised and compared against that same clock when a poll
//! reads the row back.

use af_core::id::Uuid7;

use crate::gate::decision::Answer;

/// The gate one event is waiting on.
///
/// No deadline: the row holds it, on the clock that stamped it, and the poll
/// that needs it reads that row anyway. A copy here would be a second instant
/// to drift from the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRef {
    /// The action a human was asked about.
    action_id: Uuid7,
}

impl GateRef {
    /// A reference to `action_id`.
    #[must_use]
    pub const fn new(action_id: Uuid7) -> Self {
        Self { action_id }
    }

    /// The action a human was asked about.
    #[must_use]
    pub const fn action_id(&self) -> &Uuid7 {
        &self.action_id
    }
}

/// What one poll made of a recorded gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evaluation {
    /// A human said yes; the event runs.
    Approved,
    /// A human said no; the event ends.
    Denied,
    /// No answer yet and the deadline is ahead — keep answering no-work.
    Pending,
    /// No answer and the deadline has passed; the caller resolves it timed out.
    Expired,
}

/// What one poll makes of `answer`, given whether the gate's deadline has
/// `lapsed` on the database clock.
///
/// Pure, and separated from the read that feeds it for the reason the rest
/// of this crate separates verdicts from I/O: the interesting behaviour is the
/// ordering of "answered" against "lapsed", and it needs no datastore to be
/// proven. The comparison itself is the database's — see
/// [`super::sql::SELECT_GATE_STATUS`].
///
/// An ANSWER outranks a lapsed deadline. A reviewer who approved at the last
/// second and a sweeper that has not run yet must not race to opposite
/// outcomes, and the answer is the one a human actually gave.
#[must_use]
pub const fn evaluate(answer: Option<Answer>, lapsed: bool) -> Evaluation {
    match answer {
        Some(Answer::Approved) => Evaluation::Approved,
        Some(Answer::Denied) => Evaluation::Denied,
        None if lapsed => Evaluation::Expired,
        None => Evaluation::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::{Evaluation, evaluate};
    use crate::gate::decision::Answer;

    #[test]
    fn an_answer_outranks_a_lapsed_deadline() {
        // The race this settles: a reviewer answering at the last second, and a
        // sweeper that has not run yet. Reading the deadline first would throw
        // away an answer a human actually gave.
        assert_eq!(evaluate(Some(Answer::Approved), true), Evaluation::Approved);
        assert_eq!(evaluate(Some(Answer::Denied), true), Evaluation::Denied);
    }

    #[test]
    fn no_answer_waits_until_the_database_says_the_deadline_passed() {
        assert_eq!(evaluate(None, false), Evaluation::Pending);
        assert_eq!(evaluate(None, true), Evaluation::Expired);
    }
}
