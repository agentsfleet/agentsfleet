//! The two stored vocabularies a decision arrives in.
//!
//! One is the Redis mirror's — two imperative words a human's answer is written
//! as. The other is the durable row's `status`, which is finer. They are
//! separate types because they answer different questions, and they live in one
//! file because the collapse from one to the other is the interesting part.
//!
//! # Every spelling is declared once
//!
//! Through `#[serde(rename)]` read by [`afd_core::spelling::from_spelling`],
//! never a `match` over string literals. That module carries the reasoning: a
//! hand-written match is a SECOND copy of every variant's name, and the failure
//! it causes — a row one release writes that the next cannot read — has no
//! failing test behind it.

use afd_core::spelling::from_spelling;
use serde::Deserialize;

/// The Redis mirror's word for an approval.
pub const DECISION_APPROVE: &str = "approve";

/// The Redis mirror's word for a refusal.
pub const DECISION_DENY: &str = "deny";

/// A decision, once a human has given one.
///
/// Two arms and not four: a timeout and an auto-kill both RESOLVE to a
/// refusal, and the distinction between them belongs to the durable row an
/// operator reads, not to the question "may this event run".
///
/// The two spellings are an EXTERNAL contract — the resolve path that writes
/// the mirror is the tenant plane's and is not ported here — so they are pinned
/// as explicit renames rather than derived from the variant names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Answer {
    /// A human said yes.
    #[serde(rename = "approve")]
    Approved,
    /// A human said no, or the gate lapsed into one.
    #[serde(rename = "deny")]
    Denied,
}

impl Answer {
    /// The mirror's spelling.
    ///
    /// Hand-written, unlike the read direction, because this daemon never
    /// WRITES the mirror — the only caller is a log line naming what it read.
    /// `an_answer_round_trips_through_the_mirror_spelling` is what keeps the
    /// two honest, which is the same device `runner::spelling` uses for the
    /// column spellings it writes.
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
        from_spelling(stored)
    }
}

/// The durable row's `status`, which is finer than [`Answer`].
///
/// The three refusing arms are kept apart here because the row is what an
/// operator reads and a runbook branches on — "the reviewer said no", "nobody
/// answered in time", and "the daemon stopped it" are three different
/// incidents. [`Status::answer`] is where they collapse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
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
        from_spelling(stored)
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

#[cfg(test)]
mod tests {
    use super::{Answer, Status};

    #[test]
    fn an_answer_round_trips_through_the_mirror_spelling() {
        // The read direction is serde and the write direction is hand-written,
        // so this is what holds them honest — the device `runner::spelling`
        // uses for the same split.
        for answer in [Answer::Approved, Answer::Denied] {
            assert_eq!(Answer::parse(answer.as_str()), Some(answer));
        }
        // An unrecognised mirror value leaves the gate pending rather than
        // releasing or killing the event on a byte nobody wrote. The variant
        // NAMES are deliberately not spellings.
        for unknown in ["maybe", "", "Approved", "approved", "denied"] {
            assert_eq!(Answer::parse(unknown), None, "{unknown}");
        }
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
    fn a_status_reads_the_spellings_the_column_stores() {
        for (spelling, status) in [
            ("pending", Status::Pending),
            ("approved", Status::Approved),
            ("denied", Status::Denied),
            ("timed_out", Status::TimedOut),
            ("auto_killed", Status::AutoKilled),
        ] {
            assert_eq!(Status::parse(spelling), Some(status), "{spelling}");
        }
        // A word the column never held stays unresolved, which leaves the gate
        // pending rather than guessing in either direction.
        for unknown in ["cancelled", "TimedOut", "timedOut", ""] {
            assert_eq!(Status::parse(unknown), None, "{unknown}");
        }
    }
}
