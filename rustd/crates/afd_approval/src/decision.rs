//! What an operator may write into a gate's `status` column.
//!
//! # Three arms, and `pending` is not one of them
//!
//! The runner plane's `Status` has five, because it READS the column and has to
//! recognise every state a row can be in. This type is what a decision WRITES,
//! and "resolve this to still-waiting" is not a decision — so the type cannot
//! express it, and no statement here needs a guard against it. That is the
//! whole reason the operator side does not reuse the reader's enum.
//!
//! The spellings come from [`afd_wire::approval::status`], shared with the
//! reader rather than copied: a drift between the two would make a row one
//! plane wrote the other could not read, and the gate would sit pending forever
//! with a human's answer landing nowhere.

use afd_wire::approval::status;

/// A terminal answer to an approval gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// A reviewer said yes.
    Approved,
    /// A reviewer said no.
    Denied,
    /// The deadline passed with no answer.
    ///
    /// The sweeper's, not a person's — which is why it is on this type at all:
    /// expiring a gate is writing a decision, and giving the sweeper a second
    /// path into the column would be a second place for the rule to live.
    TimedOut,
}

impl Decision {
    /// The spelling this decision stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => status::APPROVED,
            Self::Denied => status::DENIED,
            Self::TimedOut => status::TIMED_OUT,
        }
    }

    /// Whether this decision lets the blocked run continue.
    ///
    /// Only an approval does. A denial and a timeout are both the END of the
    /// run rather than a pause in it, which is why the continuation is asked
    /// this question rather than testing for `Approved` at the call site.
    #[must_use]
    pub const fn continues_the_run(self) -> bool {
        matches!(self, Self::Approved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_approval_continues_the_run() {
        assert!(Decision::Approved.continues_the_run());
        assert!(!Decision::Denied.continues_the_run());
        assert!(!Decision::TimedOut.continues_the_run());
    }

    #[test]
    fn every_decision_spells_a_status_the_reader_knows() {
        // The pairing that matters: these are the exact strings the runner
        // plane's `Status::parse` recognises, shared from one declaration.
        assert_eq!(Decision::Approved.as_str(), status::APPROVED);
        assert_eq!(Decision::Denied.as_str(), status::DENIED);
        assert_eq!(Decision::TimedOut.as_str(), status::TIMED_OUT);
        // And none of them is the state a gate waits in.
        for decision in [Decision::Approved, Decision::Denied, Decision::TimedOut] {
            assert_ne!(decision.as_str(), status::PENDING);
        }
    }
}
