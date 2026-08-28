//! A gate already raised for one event, and how a later poll resolves it.
//!
//! # No thread waits for a human
//!
//! Parking an event writes two things: a durable row, and a Redis reference
//! from the event to the action it is waiting on. Every later lease poll reads
//! that reference and re-evaluates it. There is no blocking wait, no parked
//! handler, and no timer — the deadline is a NUMBER carried in the reference,
//! compared against the caller's clock.
//!
//! # Two stores, and the fallback is the point
//!
//! A decision is written durably to Postgres and mirrored to Redis. The read
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
//! `approval_gate_async.zig` packs the reference into `"action_id|deadline_ms"`
//! and splits it back apart by hand, because Zig has no serializer and a
//! pipe-delimited pair is the cheapest thing to write. Nothing else reads that
//! key — one writer, one reader, both on the lease path — so the format was
//! never a contract, only the shape a hand-rolled encoder happened to produce.
//!
//! Here the reference IS a serde type. The separator, the split, the "what if
//! an id contains a pipe" question and the two functions that answered it are
//! all gone; what is left is a `#[serde(try_from)]` that runs the SAME domain
//! validation on every read. Likewise the two stored vocabularies below are
//! `#[serde(rename)]` declarations read through
//! [`afd_core::spelling::from_spelling`], not `match` arms — see that module
//! for why a second copy of a variant's name is the failure with no test.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

use crate::gate::decision::Answer;

/// The gate one event is waiting on.
///
/// Serialised through [`Stored`], which is what makes the identifier a real
/// [`Uuid7`] rather than "whatever thirty-six bytes were in the key". The Zig
/// carries `[36]u8` plus a length and validates only that length, so a
/// reference could name a spelling `core.fleet_approval_gates` never wrote and
/// nothing would notice until the row lookup missed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Stored", into = "Stored")]
pub struct GateRef {
    /// The action a human was asked about.
    action_id: Uuid7,
    /// When the question lapses.
    deadline: UnixMillis,
}

/// The reference as the key holds it.
///
/// A separate shape rather than serde attributes on [`GateRef`] itself,
/// because the conversion is not a rename: `action_id` arrives as a string and
/// has to PARSE, and `deadline` is a bare count of milliseconds. Routing
/// through here is what makes that validation run on every read instead of on
/// the ones a caller remembered to check.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    /// The action identifier, unvalidated until [`TryFrom`] runs.
    action_id: Box<str>,
    /// The deadline, in milliseconds since the epoch.
    deadline_ms: i64,
}

impl TryFrom<Stored> for GateRef {
    type Error = afd_core::error::Error;

    fn try_from(stored: Stored) -> Result<Self, Self::Error> {
        Ok(Self {
            action_id: Uuid7::parse(&stored.action_id)?,
            deadline: UnixMillis::from_millis(stored.deadline_ms),
        })
    }
}

impl From<GateRef> for Stored {
    fn from(reference: GateRef) -> Self {
        Self {
            action_id: reference.action_id.as_str().into(),
            deadline_ms: reference.deadline.as_millis(),
        }
    }
}

impl GateRef {
    /// A reference to `action_id`, lapsing at `deadline`.
    #[must_use]
    pub const fn new(action_id: Uuid7, deadline: UnixMillis) -> Self {
        Self {
            action_id,
            deadline,
        }
    }

    /// The action a human was asked about.
    #[must_use]
    pub const fn action_id(&self) -> &Uuid7 {
        &self.action_id
    }

    /// When the question lapses.
    #[must_use]
    pub const fn deadline(&self) -> UnixMillis {
        self.deadline
    }

    /// Whether `now` is past the deadline.
    #[must_use]
    pub const fn has_lapsed(&self, now: UnixMillis) -> bool {
        now.as_millis() > self.deadline.as_millis()
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

/// What one poll makes of `answer` against `reference` at `now`.
///
/// Pure, and separated from the two reads that feed it for the reason the rest
/// of this crate separates verdicts from I/O: the interesting behaviour is the
/// deadline comparison and the ordering of "answered" against "lapsed", and
/// neither needs a datastore to be proven.
///
/// An ANSWER outranks a lapsed deadline. A reviewer who approved at the last
/// second and a sweeper that has not run yet must not race to opposite
/// outcomes, and the answer is the one a human actually gave.
#[must_use]
pub fn evaluate(reference: &GateRef, answer: Option<Answer>, now: UnixMillis) -> Evaluation {
    match answer {
        Some(Answer::Approved) => Evaluation::Approved,
        Some(Answer::Denied) => Evaluation::Denied,
        None if reference.has_lapsed(now) => Evaluation::Expired,
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
    use afd_core::clock::UnixMillis;
    use afd_core::id::Uuid7;

    const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";
    const DEADLINE: i64 = 1_765_000_000_000;

    fn reference() -> GateRef {
        GateRef::new(
            Uuid7::parse(ACTION).expect("a canonical identifier"),
            UnixMillis::from_millis(DEADLINE),
        )
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
        // The stored shape is named fields, so a reader — or a `redis-cli` —
        // sees what each number is rather than a position in a delimited pair.
        assert!(written.contains("action_id"), "{written}");
        assert!(written.contains("deadline_ms"), "{written}");
    }

    #[test]
    fn a_reference_that_does_not_validate_is_refused_on_read() {
        // The `try_from` runs on EVERY deserialize, so an identifier that is
        // the right length and the wrong shape cannot become a `GateRef` — the
        // hand-rolled version checked only that it was at most thirty-six
        // bytes.
        for refused in [
            r#"{"action_id":"aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa","deadline_ms":1}"#,
            r#"{"action_id":"0193E9A0-0000-7000-8000-00000000AAAA","deadline_ms":1}"#,
            r#"{"action_id":"","deadline_ms":1}"#,
            r#"{"action_id":"not-an-id","deadline_ms":1}"#,
            // Serde carries the rest for free: a missing field, a wrong type,
            // and a value that is not an object at all.
            r#"{"deadline_ms":1}"#,
            r#"{"action_id":"0193e9a0-0000-7000-8000-00000000aaaa"}"#,
            r#"{"action_id":"0193e9a0-0000-7000-8000-00000000aaaa","deadline_ms":"soon"}"#,
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
        let long_past = UnixMillis::from_millis(DEADLINE + 60_000);

        assert_eq!(
            evaluate(&reference(), Some(Answer::Approved), long_past),
            Evaluation::Approved
        );
        assert_eq!(
            evaluate(&reference(), Some(Answer::Denied), long_past),
            Evaluation::Denied
        );
    }

    #[test]
    fn no_answer_waits_until_the_deadline_and_then_expires() {
        let reference = reference();

        assert_eq!(
            evaluate(&reference, None, UnixMillis::from_millis(DEADLINE - 1)),
            Evaluation::Pending
        );
        // The boundary is exclusive — at exactly the deadline it is still
        // pending, which is the Zig's `now_ms > deadline_ms`.
        assert_eq!(
            evaluate(&reference, None, UnixMillis::from_millis(DEADLINE)),
            Evaluation::Pending
        );
        assert_eq!(
            evaluate(&reference, None, UnixMillis::from_millis(DEADLINE + 1)),
            Evaluation::Expired
        );
    }
}
