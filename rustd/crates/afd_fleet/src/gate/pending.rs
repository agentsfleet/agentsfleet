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

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;

/// What separates the action from its deadline in the stored reference.
const REF_SEPARATOR: char = '|';

/// The Redis mirror's word for an approval.
pub const DECISION_APPROVE: &str = "approve";

/// The Redis mirror's word for a refusal.
pub const DECISION_DENY: &str = "deny";

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

impl Answer {
    /// The mirror's spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => DECISION_APPROVE,
            Self::Denied => DECISION_DENY,
        }
    }

    /// Recover an answer from the mirror.
    ///
    /// `None` for anything else, which is the fail-safe direction: an
    /// unrecognised mirror value leaves the gate PENDING rather than releasing
    /// or killing the event on a byte nobody wrote.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            DECISION_APPROVE => Some(Self::Approved),
            DECISION_DENY => Some(Self::Denied),
            _unknown => None,
        }
    }
}

/// The durable row's `status`, which is finer than [`Answer`].
///
/// The three refusing arms are kept apart here because the row is what an
/// operator reads and a runbook branches on — "the reviewer said no", "nobody
/// answered in time", and "the daemon stopped it" are three different
/// incidents. [`Status::answer`] is where they collapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Still waiting on a human.
    Pending,
    /// A reviewer approved it.
    Approved,
    /// A reviewer refused it.
    Denied,
    /// The deadline passed with no answer.
    TimedOut,
    /// The daemon stopped the fleet.
    AutoKilled,
}

impl Status {
    /// Recover a status from its stored spelling.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        match stored {
            "pending" => Some(Self::Pending),
            "approved" => Some(Self::Approved),
            "denied" => Some(Self::Denied),
            "timed_out" => Some(Self::TimedOut),
            "auto_killed" => Some(Self::AutoKilled),
            _unknown => None,
        }
    }

    /// The answer this status resolves to, if it resolves to one at all.
    ///
    /// `None` is [`Status::Pending`] and only that — which is why the durable
    /// read can return an `Option` rather than needing a caller to remember
    /// that pending is not terminal.
    #[must_use]
    pub const fn answer(self) -> Option<Answer> {
        match self {
            Self::Pending => None,
            Self::Approved => Some(Answer::Approved),
            Self::Denied | Self::TimedOut | Self::AutoKilled => Some(Answer::Denied),
        }
    }
}

/// The gate one event is waiting on.
///
/// The action id is a [`Uuid7`] rather than a bounded byte buffer. The Zig
/// carries `[36]u8` plus a length and validates only that length, which admits
/// any thirty-six bytes — including a spelling `core.fleet_approval_gates`
/// could never have written. Parsing to the identifier type instead means a
/// `GateRef` that exists names something that could be a row, and the read that
/// follows needs no second check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateRef {
    /// The action a human was asked about.
    action_id: Uuid7,
    /// When the question lapses.
    deadline: UnixMillis,
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

    /// The stored form: the action, a separator, the deadline.
    #[must_use]
    pub fn encode(&self) -> String {
        format!(
            "{}{REF_SEPARATOR}{}",
            self.action_id.as_str(),
            self.deadline.as_millis()
        )
    }

    /// Recover a reference from its stored form.
    ///
    /// `None` for anything that is not one. Every rejection here leaves the
    /// event looking UNPARKED to the caller, which routes to raising a fresh
    /// gate — so this is deliberately strict: a reference nobody can read is
    /// better replaced than half-honoured.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let (action, deadline) = raw.split_once(REF_SEPARATOR)?;
        Some(Self {
            action_id: Uuid7::parse(action).ok()?,
            deadline: UnixMillis::from_millis(deadline.parse().ok()?),
        })
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
    use super::{Answer, Evaluation, GateRef, Status, evaluate};
    use afd_core::clock::UnixMillis;
    use afd_core::id::Uuid7;

    const ACTION: &str = "0193e9a0-0000-7000-8000-00000000aaaa";
    const DEADLINE: i64 = 1_765_000_000_000;

    fn reference() -> GateRef {
        GateRef::parse(&format!("{ACTION}|{DEADLINE}")).expect("a well-formed reference")
    }

    #[test]
    fn a_reference_round_trips_through_its_stored_form() {
        let parsed = reference();

        assert_eq!(parsed.action_id().as_str(), ACTION);
        assert_eq!(parsed.deadline().as_millis(), DEADLINE);
        assert_eq!(parsed.encode(), format!("{ACTION}|{DEADLINE}"));
        assert_eq!(GateRef::parse(&parsed.encode()), Some(parsed));
    }

    #[test]
    fn a_malformed_reference_is_refused_rather_than_half_read() {
        for refused in [
            "",
            "no-separator",
            "|123",
            &format!("{ACTION}|not-a-number"),
            "this-action-id-is-way-too-long-to-be-a-uuid-string|1",
            // Thirty-six bytes, and not an identifier this daemon ever wrote.
            "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa|1",
            // Upper case: canonical for a UUID, not for THIS product's ids.
            &format!("{}|1", ACTION.to_uppercase()),
        ] {
            assert_eq!(GateRef::parse(refused), None, "{refused}");
        }
    }

    #[test]
    fn an_answer_round_trips_through_the_mirror_spelling() {
        for answer in [Answer::Approved, Answer::Denied] {
            assert_eq!(Answer::parse(answer.as_str()), Some(answer));
        }
        // An unrecognised mirror value leaves the gate pending rather than
        // releasing or killing the event on a byte nobody wrote.
        assert_eq!(Answer::parse("maybe"), None);
        assert_eq!(Answer::parse(""), None);
    }

    #[test]
    fn every_refusing_status_resolves_to_one_denial() {
        assert_eq!(Status::Approved.answer(), Some(Answer::Approved));
        for refusing in [Status::Denied, Status::TimedOut, Status::AutoKilled] {
            assert_eq!(refusing.answer(), Some(Answer::Denied), "{refusing:?}");
        }
        // And pending is the only status with no answer, which is what lets the
        // durable read hand back an `Option` instead of a terminality check.
        assert_eq!(Status::Pending.answer(), None);
    }

    #[test]
    fn a_status_round_trips_through_its_stored_spelling() {
        for status in [
            Status::Pending,
            Status::Approved,
            Status::Denied,
            Status::TimedOut,
            Status::AutoKilled,
        ] {
            let spelling = match status {
                Status::Pending => "pending",
                Status::Approved => "approved",
                Status::Denied => "denied",
                Status::TimedOut => "timed_out",
                Status::AutoKilled => "auto_killed",
            };
            assert_eq!(Status::parse(spelling), Some(status));
        }
        assert_eq!(Status::parse("cancelled"), None);
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

    #[test]
    fn a_reference_can_be_built_without_parsing_one() {
        // The write side builds these; the read side parses them. Both have to
        // produce the same value or a parked event cannot find its own gate.
        let built = GateRef::new(
            Uuid7::parse(ACTION).expect("a canonical identifier"),
            UnixMillis::from_millis(DEADLINE),
        );

        assert_eq!(built, reference());
    }
}
