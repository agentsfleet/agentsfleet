//! What an operator may write into a gate's `status` column.
//!
//! # Three arms, and `pending` is not one of them
//!
//! The runner plane's `Status` has more, because it READS the column and has
//! to recognise every state a row can be in. This type is what a decision
//! WRITES, and "resolve this to still-waiting" is not a decision — so the type
//! cannot express it, and no statement here needs a guard against it. That is
//! the whole reason the operator side does not reuse the reader's enum.
//!
//! `timed_out` is not one either: nothing expires a gate. A read derives it
//! for an unanswered gate past its deadline, and the gate stays answerable.
//!
//! The spellings come from [`afd_api_wire::approval::status`], shared with the
//! reader rather than copied: a drift between the two would make a row one
//! plane wrote the other could not read, and the gate would sit pending forever
//! with a human's answer landing nowhere.

use afd_api_wire::approval::status;

/// A terminal answer to an approval gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// A reviewer said yes.
    Approved,
    /// A reviewer said no.
    Denied,
    /// A person answered a run's question; the answer is the gate's detail.
    Answered,
}

impl Decision {
    /// The spelling this decision stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Approved => status::APPROVED,
            Self::Denied => status::DENIED,
            Self::Answered => status::ANSWERED,
        }
    }

    /// Whether this decision lets the blocked run continue.
    ///
    /// An approval and an answer do. A denial is the END of the run rather
    /// than a pause in it, which is why the continuation is asked this
    /// question rather than testing for `Approved` at the call site.
    #[must_use]
    pub const fn continues_the_run(self) -> bool {
        matches!(self, Self::Approved | Self::Answered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_approval_and_an_answer_continue_the_run() {
        assert!(Decision::Approved.continues_the_run());
        assert!(Decision::Answered.continues_the_run());
        assert!(!Decision::Denied.continues_the_run());
    }

    #[test]
    fn every_decision_spells_a_status_the_reader_knows() {
        // The pairing that matters: these are the exact strings the runner
        // plane's `Status::parse` recognises, shared from one declaration.
        assert_eq!(Decision::Approved.as_str(), status::APPROVED);
        assert_eq!(Decision::Denied.as_str(), status::DENIED);
        assert_eq!(Decision::Answered.as_str(), status::ANSWERED);
        // And none of them is the state a gate waits in, nor the one a read
        // derives for a gate past its deadline.
        for decision in [Decision::Approved, Decision::Denied, Decision::Answered] {
            assert_ne!(decision.as_str(), status::PENDING);
            assert_ne!(decision.as_str(), status::TIMED_OUT);
        }
    }
}
