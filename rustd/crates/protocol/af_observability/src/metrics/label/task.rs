//! Closed task diagnostics; private identifiers belong in traces, never labels.

use crate::metrics::label::closed_set;

closed_set! {
    /// The native lifecycle operation being measured.
    TaskOperation {
        /// Atomically accept or replay a complete interpretation.
        Bind => "bind",
        /// Accept explicitly reviewed completion wording for existing work.
        AdoptGoal => "adopt_goal",
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
        /// Acknowledge accepted input at its original delivery.
        InputReceipt => "input_receipt",
        /// Allocate and issue one native exact-head publication.
        EffectPublish => "effect_publish",
        /// Reconcile/read immutable publication history.
        EffectRead => "effect_read",
        /// Allocate a complete checkpoint plan.
        CheckpointPlan => "checkpoint_plan",
        /// Read or freeze an immutable bounded manifest page.
        CheckpointPage => "checkpoint_page",
        /// Reserve unique native object ownership and quota.
        CheckpointRegister => "checkpoint_register",
        /// Authorize, verify or page private object transfers.
        CheckpointTransfer => "checkpoint_transfer",
        /// Renew capture or restore ownership.
        CheckpointPin => "checkpoint_pin",
        /// Commit a complete recoverable generation.
        CheckpointCommit => "checkpoint_commit",
        /// Retain private owners and reconcile remote purges.
        CheckpointSweep => "checkpoint_sweep",
        /// Prepare recorded goal and independently read dependency evidence.
        ReportPrepare => "report_prepare",
        /// Commit a native task disposition beside its financial report.
        ReportCommit => "report_commit",
        /// Retain and match a verified workflow completion.
        CompletionMatch => "completion_match",
    }
}

closed_set! {
    /// Caller-visible result of a native task operation.
    TaskOutcome {
        /// The guarded operation succeeded.
        Accepted => "accepted",
        /// A previously committed receipt was returned unchanged.
        Replayed => "replayed",
        /// No task or run is visible in the authorized scope.
        NotFound => "not_found",
        /// Version or run ownership changed.
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
