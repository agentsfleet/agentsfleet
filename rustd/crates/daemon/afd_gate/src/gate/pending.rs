//! A gate already raised for one event, and how a later poll resolves it.
//!
//! # No thread waits for a human
//!
//! Parking an event writes a durable `fleet.approval_gates` row naming the
//! action it is waiting on. Every later run poll finds that row and
//! re-evaluates it. There is no blocking wait, no parked handler, and no timer.
//! The row's `expires_at` is a deadline a card shows; an unanswered gate stays
//! answerable past it, and the event waits until a person answers.

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
///
/// No arm for a lapsed deadline: an unanswered gate stays answerable past
/// it, so the event keeps waiting and the deadline is only what a card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evaluation {
    /// A human said yes; the event runs.
    Approved,
    /// A human said no; the event ends.
    Denied,
    /// No answer yet — keep answering no-work.
    Pending,
}

/// What one poll makes of `answer`.
#[must_use]
pub const fn evaluate(answer: Option<Answer>) -> Evaluation {
    match answer {
        Some(Answer::Approved) => Evaluation::Approved,
        Some(Answer::Denied) => Evaluation::Denied,
        None => Evaluation::Pending,
    }
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
    /// `status` is absent when the reference's row is gone, which only a
    /// purge does: the gate is pending, the fail-safe direction. A status
    /// this build cannot spell is no answer, as the store reads it.
    #[must_use]
    pub fn observed(action_id: Option<&Uuid7>, status: Option<&str>) -> Self {
        if action_id.is_none() {
            return Self::Absent;
        }
        let answer = status
            .and_then(af_core::spelling::from_spelling::<afd_api_wire::approval::GateStatus>)
            .and_then(crate::gate::decision::answer);
        Self::Held(evaluate(answer))
    }
}

#[cfg(test)]
mod tests {
    use super::{Evaluation, evaluate};
    use crate::gate::decision::Answer;

    #[test]
    fn an_answer_decides_and_no_answer_waits_whatever_the_deadline() {
        assert_eq!(evaluate(Some(Answer::Approved)), Evaluation::Approved);
        assert_eq!(evaluate(Some(Answer::Denied)), Evaluation::Denied);
        assert_eq!(evaluate(None), Evaluation::Pending);
    }
}
