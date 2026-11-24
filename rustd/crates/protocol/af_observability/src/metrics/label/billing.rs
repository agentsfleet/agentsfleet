//! What the billing families label their counts with.
//!
//! `WorkType` spells the stored values of `billing.usage_lines` byte for byte;
//! `afd_billing` depends on this crate, so the two cannot share a type, and a
//! test there holds them equal member for member.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which product's work a billing count is about.
    WorkType {
        /// A fleet run's run.
        FleetRun => "fleet_run",
    }
}

closed_set! {
    /// What a funds check answered.
    FundsOutcome {
        /// The balance covers the ask.
        Covered => "covered",
        /// It could not cover it.
        Short => "short",
        /// The biller could not answer and the outage policy admitted the work.
        Unconfirmed => "unconfirmed",
    }
}
