//! Every state a gate's column can be in, as a READER sees it.
//!
//! # Why this is not [`crate::Decision`]
//!
//! `Decision` is what an operator WRITES, and it carries three arms on purpose:
//! "resolve this to still-waiting" is not a decision, and neither is the
//! killer's verdict. A filter asks a different question — which rows to show —
//! and the answer has to name every state a row can be in, or a status the
//! column really holds becomes unreachable through the API. That is what
//! happened: an inbox filter routed through `Decision` served four of the five
//! spellings and refused `auto_killed`, which the dashboard's own type
//! declares.
//!
//! The spellings come from [`afd_wire::approval::status`], shared with the
//! writer rather than copied, for the reason `Decision` gives about the same
//! table: a drift between the two would make a row one plane wrote the other
//! could not name.

use afd_wire::approval::status;

/// One state a gate row can be in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateStatus {
    /// Raised and waiting for an answer.
    Pending,
    /// A reviewer said yes.
    Approved,
    /// A reviewer said no.
    Denied,
    /// The deadline passed with no answer.
    TimedOut,
    /// The platform's anomaly gate ended the run.
    AutoKilled,
}

impl GateStatus {
    /// The spelling the column stores.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => status::PENDING,
            Self::Approved => status::APPROVED,
            Self::Denied => status::DENIED,
            Self::TimedOut => status::TIMED_OUT,
            Self::AutoKilled => status::AUTO_KILLED,
        }
    }

    /// The state `raw` names, or `None` for a spelling no row carries.
    ///
    /// Answering `None` rather than a default is what lets the edge refuse an
    /// unknown status instead of quietly serving the pending page — which is
    /// what an ignored filter amounts to, and what a caller reads as an empty
    /// inbox.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            status::PENDING => Some(Self::Pending),
            status::APPROVED => Some(Self::Approved),
            status::DENIED => Some(Self::Denied),
            status::TIMED_OUT => Some(Self::TimedOut),
            status::AUTO_KILLED => Some(Self::AutoKilled),
            _unknown => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spelling_the_column_holds_round_trips() {
        for state in [
            GateStatus::Pending,
            GateStatus::Approved,
            GateStatus::Denied,
            GateStatus::TimedOut,
            GateStatus::AutoKilled,
        ] {
            assert_eq!(GateStatus::parse(state.as_str()), Some(state));
        }
    }

    #[test]
    fn the_killers_verdict_is_readable_and_still_not_writable() {
        // The gap this type exists to close: a reader can name it, and
        // `Decision` still cannot, so no operator can write it by hand.
        assert_eq!(
            GateStatus::parse(status::AUTO_KILLED),
            Some(GateStatus::AutoKilled)
        );
        assert_eq!(GateStatus::parse("elsewhere"), None);
    }
}
