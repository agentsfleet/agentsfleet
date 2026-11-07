//! What the cost families attribute a run's spend to.
//!
//! These are not in the census's `labels` column and do not count against a
//! fixed ceiling: the cost families draw on the shared cost sub-budget, whose
//! size is decided by how many exact `(provider, model)` pairs may carry
//! attribution at once. What they still need is a closed spelling, because a
//! label written two ways is two series in the money dashboards.

use crate::metrics::label::closed_set;

closed_set! {
    /// Which direction a token count is.
    ///
    /// Two, not three: input ALREADY includes the cached portion, and the
    /// cache-read subset has a family of its own. A third additive direction
    /// would make the total wrong for anyone who summed them.
    TokenType {
        /// Prompt tokens, cached ones included.
        Input => "input",
        /// Completion tokens.
        Output => "output",
    }
}

closed_set! {
    /// Which class of debit a charge was.
    ///
    /// One class since every charge became a settlement of usage lines
    /// through the biller (billing §11): a renewal's slice and a report's final
    /// slice settle the same way, and the biller that records the spend cannot
    /// tell them apart. Kept as a set so a second class has one place to land.
    ChargeClass {
        /// Charged when usage lines settled.
        Settle => "settle",
    }
}

closed_set! {
    /// The coarse verdict on the agent-duration histogram.
    ///
    /// Absent on a clean run, and one member otherwise. The granular failure
    /// class stays OFF this family on purpose: it multiplies the per-model
    /// series budget, and it is already carried exactly by the durable event
    /// row and by the capped per-runner failure counter.
    ErrorType {
        /// The run failed.
        FleetError => "fleet_error",
    }
}
