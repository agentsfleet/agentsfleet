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
    evaluate_answer(answer, lapsed)
}

/// What a caller's read found about the gate an event already holds.
///
/// Built from a statement that read the reference and its newest row in one
/// pass (`afd_fleet`'s gates read), so [`super::Gates::judge`] decides
/// without reading either again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recorded {
    /// The event holds no gate.
    Absent,
    /// The event holds a gate, evaluated from its newest row.
    Held(Evaluation),
    /// The read failed; whether a gate is held is unknown.
    Unreadable(af_core::error_code::Logged),
}

impl Recorded {
    /// What a read of the reference and its newest row returned.
    ///
    /// `status` and `lapsed` are absent when the reference's row is gone,
    /// which only a purge does: the gate is pending, the fail-safe direction.
    /// A status this build cannot spell is no answer, as the store reads it.
    #[must_use]
    pub fn observed(
        action_id: Option<&af_core::id::Uuid7>,
        status: Option<&str>,
        lapsed: Option<bool>,
    ) -> Self {
        if action_id.is_none() {
            return Self::Absent;
        }
        let Some(lapsed) = lapsed else {
            return Self::Held(Evaluation::Pending);
        };
        let answer = status
            .and_then(af_core::spelling::from_spelling::<afd_api_wire::approval::GateStatus>)
            .and_then(crate::gate::decision::answer);
        Self::Held(evaluate_answer(answer, lapsed))
    }
}

const fn evaluate_answer(answer: Option<Answer>, lapsed: bool) -> Evaluation {
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
