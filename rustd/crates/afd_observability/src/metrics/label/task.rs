//! Closed task diagnostics; private identifiers belong in traces, never labels.

use crate::metrics::label::closed_set;

closed_set! {
    /// The native lifecycle operation being measured.
    TaskOperation {
        /// Atomically accept or replay a complete interpretation.
        Bind => "bind",
        /// Read an authorized task snapshot.
        Read => "read",
        /// Suspend future work.
        Pause => "pause",
        /// Make stopped work eligible again.
        Resume => "resume",
        /// Fence future task effects.
        Cancel => "cancel",
        /// Reserve funding and issue one physical decision credential.
        DecisionReserve => "decision_reserve",
        /// Record one canonical result or retained accounting uncertainty.
        DecisionRecord => "decision_record",
        /// Release reservations natively proved unsent after their deadline.
        DecisionSweep => "decision_sweep",
    }
}

closed_set! {
    /// Result of one completed recovery attempt, without task identifiers.
    DecisionRecoveryOutcome {
        /// Native proof released a previously unsent reservation.
        Expired => "expired",
        /// Another writer changed or removed the discovered candidate.
        Obsolete => "obsolete",
        /// Existing financial locks were busy; a later scan retries.
        Contended => "contended",
        /// A row-local failure retained uncertainty for operator repair.
        Failed => "failed",
    }
}

closed_set! {
    /// Caller-visible result of a native task operation.
    TaskOutcome {
        /// The guarded operation succeeded.
        Accepted => "accepted",
        /// A previously committed receipt was returned unchanged.
        Replayed => "replayed",
        /// No task or lease is visible in the authorized scope.
        NotFound => "not_found",
        /// Version or lease ownership changed.
        Stale => "stale",
        /// Native target validation refused the proposal.
        Invalid => "invalid",
        /// The immutable interpretation was changed on replay.
        Conflict => "conflict",
        /// Capacity or a cumulative allowance is exhausted.
        Limit => "limit",
        /// A required prerequisite or dependency is unavailable.
        Unavailable => "unavailable",
        /// An acquisition, query or connected read exceeded its native deadline.
        Timeout => "timeout",
        /// The caller dropped the operation before a result was returned.
        Cancelled => "cancelled",
        /// Another native error prevented completion.
        Failed => "failed",
    }
}
