//! What the billing families label their counts with.
//!
//! `WorkType` spells the stored values of `billing.usage_lines` byte for byte;
//! `afd_billing` depends on this crate, so the two cannot share a type, and a
//! test there holds them equal member for member.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which product's work a billing count is about.
    WorkType {
        /// A fleet run's lease.
        FleetLease => "fleet_lease",
        /// A setup-chat attempt.
        AuthoringAttempt => "authoring_attempt",
    }
}

closed_set! {
    /// What a funds check answered.
    FundsOutcome {
        /// The balance, less what is logged and not yet charged, covers the ask.
        Covered => "covered",
        /// It could not cover it.
        Short => "short",
        /// The biller could not answer and the outage policy admitted the work.
        Unconfirmed => "unconfirmed",
    }
}

closed_set! {
    /// Whether a settlement landed, and if not, whose fault it was.
    ///
    /// Two failures, because their cures differ: an outage clears on its own
    /// and the sweeper retries it, where a defect retries into the same refusal
    /// until someone fixes it.
    SettleOutcome {
        /// Charged.
        Ok => "ok",
        /// The datastore could not be reached; the lines wait for the sweeper.
        DatastoreUnavailable => "datastore_unavailable",
        /// Refused on its own terms; the lines wait for a fix.
        Defect => "defect",
    }
}
