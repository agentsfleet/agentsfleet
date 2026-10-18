//! A gate already raised for one event, and how a later poll resolves it.
//!
//! # No thread waits for a human
//!
//! Parking an event writes two things: a durable row, and a Dragonfly reference
//! from the event to the action it is waiting on. Every later lease poll reads
//! that reference and re-evaluates it. There is no blocking wait, no parked
//! handler, and no timer — the deadline is the row's `expires_at`, stamped by
//! the database clock when the gate was raised and compared against that same
//! clock when a poll reads the row back.
//!
//! # Two stores, and the fallback is the point
//!
//! A decision is written durably to Postgres and mirrored to Dragonfly. The read
//! prefers the mirror, because it is one round trip and it is the hot path —
//! but it falls back to the durable row when the mirror key is absent, so a
//! committed decision is enforced even if the best-effort mirror write failed
//! after the commit. Without the fallback, a resolve that lost its mirror write
//! leaves a human's answer un-honoured until the reference expires.
//!
//! That fallback is instrumented rather than silent: it fires only in the
//! window the mirror write missed, so it is the metric that says the write side
//! has a gap.
//!
//! # Every spelling here is declared once
//!
//! The reference IS a serde type, so there is no separator to choose, no
//! hand-written split, and no question of what an id containing the separator
//! does: a `#[serde(try_from)]` runs the SAME domain validation on every read.
//! One writer and one reader touch that key, both on the lease path.
//!
//! Likewise the two stored vocabularies below are `#[serde(rename)]`
//! declarations read through
//! [`afd_core::spelling::from_spelling`], not `match` arms — see that module
//! for why a second copy of a variant's name is the failure with no test.

use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

use crate::gate::decision::Answer;

/// The gate one event is waiting on.
///
/// Serialised through [`Stored`], which is what makes the identifier a real
/// [`Uuid7`] rather than "whatever thirty-six bytes were in the key". A
/// reference checked only for its length could name a spelling
/// `fleet.approval_gates` never wrote, and nothing would notice until the
/// row lookup missed.
///
/// No deadline: the row holds it, on the clock that stamped it, and the poll
/// that needs it reads that row anyway. A copy here would be a second instant
/// to drift from the first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub struct GateRef {
    /// The action a human was asked about.
    action_id: Uuid7,
}

/// The reference as the key holds it.
///
/// A separate shape rather than serde attributes on [`GateRef`] itself,
/// because the conversion is not a rename: `action_id` arrives as a string and
/// has to PARSE. Routing through here is what makes that validation run on
/// every read instead of on the ones a caller remembered to check.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    /// The action identifier, unvalidated until [`TryFrom`] runs.
    action_id: Box<str>,
}

impl TryFrom<Stored> for GateRef {
    type Error = afd_core::error::Error;

    fn try_from(stored: Stored) -> Result<Self, Self::Error> {
        Ok(Self {
            action_id: Uuid7::parse(&stored.action_id)?,
        })
    }
}

impl From<GateRef> for Stored {
    fn from(reference: GateRef) -> Self {
        Self {
            action_id: reference.action_id.as_str().into(),
        }
    }
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
/// Pure, and separated from the two reads that feed it for the reason the rest
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
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
    )]
    use super::{Evaluation, GateRef, evaluate};
    use crate::gate::decision::Answer;
    use afd_core::id::Uuid7;

    const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";

    fn reference() -> GateRef {
        GateRef::new(Uuid7::parse(ACTION).expect("a canonical identifier"))
    }

    fn stored(reference: &GateRef) -> String {
        serde_json::to_string(reference).expect("a reference serialises")
    }

    #[test]
    fn a_reference_round_trips_through_its_stored_form() {
        let written = stored(&reference());

        assert_eq!(
            serde_json::from_str::<GateRef>(&written).expect("what we wrote reads back"),
            reference()
        );
        // The stored shape is a named field, so a reader — or a `redis-cli` —
        // sees what the value is, and it carries no instant of its own.
        assert_eq!(written, format!(r#"{{"action_id":"{ACTION}"}}"#));
    }

    #[test]
    fn a_reference_that_does_not_validate_is_refused_on_read() {
        // The `try_from` runs on EVERY deserialize, so an identifier that is
        // the right length and the wrong shape cannot become a `GateRef`.
        for refused in [
            r#"{"action_id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa"}"#,
            r#"{"action_id":"0193E9A0-0000-7000-8000-00000000AAAA"}"#,
            r#"{"action_id":""}"#,
            r#"{"action_id":"not-an-id"}"#,
            // Serde carries the rest for free: a missing field, a wrong type,
            // and a value that is not an object at all.
            "{}",
            r#"{"action_id":7}"#,
            "[]",
            "",
        ] {
            assert!(
                serde_json::from_str::<GateRef>(refused).is_err(),
                "{refused}"
            );
        }
    }

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
