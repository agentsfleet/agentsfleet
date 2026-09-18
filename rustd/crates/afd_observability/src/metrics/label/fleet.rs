//! What the account, repair and verification paths label their outcomes with.

use crate::metrics::label::closed_set;

/// This one has been seen before, and the first delivery's answer stands.
///
/// One spelling for one fact, shared by the two sets that observe it: a
/// provider result can arrive twice, and so can the event a verification
/// produces. Named rather than written twice so the two cannot be renamed
/// apart while still meaning the same thing.
const REPLAYED: &str = "replayed";

/// The entry reached the stream on this pass.
///
/// Shared by the two sets that observe it for the same reason [`REPLAYED`] is:
/// a first admission and a sweeper's re-append are different events with the
/// same outcome, and a dashboard summing them has to see one spelling.
const APPENDED: &str = "appended";

closed_set! {
    /// Which side of the platform boundary a failed run fell on.
    ///
    /// An error budget is a promise about what this control plane controls. A
    /// run that died because its workload asked for something policy refuses,
    /// outgrew a ceiling, or ran out of money is a run this platform DELIVERED
    /// correctly, and counting it against the objective lets one tenant's bad
    /// code spend everybody's budget.
    ///
    /// Two members and not eleven: the eleven are `FailureClass`, which stays
    /// the operator's diagnostic on `agentsfleet_runner_failures_total`. This
    /// set answers the only question the objective asks, and keeps the series
    /// count on the executions family at three rather than twelve.
    Fault {
        /// Ours. It spends the error budget.
        Platform => "platform",
        /// The workload's. The platform delivered; what it ran did not.
        Workload => "workload",
    }
}

closed_set! {
    /// Why opening an account from a signup delivery did not happen.
    ///
    /// Six, and the count is the point: the first three are the delivery being
    /// wrong and the last three are this daemon being unable to act on a
    /// delivery that was right. An operator seeing a spike needs to know which
    /// half, because only one of them is theirs to fix.
    SignupFailure {
        /// The signature did not verify.
        BadSignature => "bad_sig",
        /// The delivery's timestamp is outside the replay window.
        StaleTimestamp => "stale_ts",
        /// The payload carried no address to open an account against.
        MissingEmail => "missing_email",
        /// The database refused the write.
        DatabaseError => "db_error",
        /// No connection was available to attempt it on.
        ///
        /// The daemon this ports separates this from a database refusal; here
        /// an exhausted pool arrives as one, so nothing writes this yet. It
        /// stays in the set because the set is the WIRE vocabulary both
        /// binaries share during the cutover, and the census ceiling is derived
        /// from it — dropping it would understate the budget for a value the
        /// other daemon still emits.
        PoolUnavailable => "pool_unavailable",
        /// The account opened and the provider would not record that it had.
        ///
        /// Kept for [`SignupFailure::PoolUnavailable`]'s reason: this daemon
        /// writes no metadata back, and the other one does.
        MetadataWriteback => "metadata_writeback",
    }
}

closed_set! {
    /// What became of one inbound provider result.
    ProviderResult {
        /// Taken, and it produced a repair.
        Accepted => "accepted",
        /// Seen before; the first delivery's answer stands.
        Replayed => REPLAYED,
        /// Dropped because it normalises to nothing this daemon acts on.
        IgnoredNormalization => "ignored_normalization",
        /// Dropped because it names a repository this deployment does not hold.
        IgnoredRepository => "ignored_repository",
    }
}

closed_set! {
    /// Whether a result could be tied to the repair that caused it.
    ///
    /// `Ambiguous` is its own member rather than folded into `Missed`: a result
    /// matching several repairs is a correlation this daemon declines to guess
    /// at, and one matching none is a result it has no repair for. They are
    /// different investigations.
    Correlation {
        /// Exactly one repair matched.
        Matched => "matched",
        /// No repair matched.
        Missed => "missed",
        /// More than one matched, so none was chosen.
        Ambiguous => "ambiguous",
    }
}

closed_set! {
    /// Whether a verification event was appended or already there.
    SyntheticEvent {
        /// Appended by this pass.
        Emitted => "emitted",
        /// The append-once key answered with an earlier pass's event.
        Replayed => REPLAYED,
    }
}

closed_set! {
    /// What became of one admission.
    ///
    /// Five, and the split is the one an operator needs: `Deferred` is work
    /// this daemon ACCEPTED and could not queue — the row is safe and the
    /// replay sweeper owes it an entry — where `Refused` is work it did not
    /// accept at all, and `OverBudget` is work it refused ON PURPOSE because
    /// a fleet or the deployment holds as much as it is allowed to. A single
    /// "failed" would hide the difference between a queue outage nobody loses
    /// work to, a database outage a producer must retry through, and a
    /// deployment doing exactly what its budgets say.
    AdmissionOutcome {
        /// Committed and appended.
        Appended => APPENDED,
        /// Seen before; the first admission's id stands.
        Replayed => REPLAYED,
        /// Committed; the queue would not take the entry yet.
        Deferred => "deferred",
        /// Not committed, and the producer was told so.
        Refused => "refused",
        /// Not committed because a budget is spent; the producer backs off.
        OverBudget => "over_budget",
    }
}

closed_set! {
    /// What became of one replayed admission.
    ///
    /// `Full` is its own member because its cure is the opposite of
    /// `Failed`'s: a queue that is gone wants the pass to come back, and one
    /// that is full wants everything to stop until something drains.
    ReplayOutcome {
        /// Re-appended, and the receipt recorded.
        Appended => APPENDED,
        /// The queue would not take it; the row keeps its NULL receipt.
        Failed => "failed",
        /// The queue refused to grow; the row keeps its NULL receipt.
        Full => "full",
    }
}

closed_set! {
    /// Where a verification run got to.
    VerifierRun {
        /// Dispatched onto the fleet's stream.
        Queued => "queued",
        /// Recorded as having produced its event.
        Completed => "completed",
    }
}

closed_set! {
    /// Where a fleet stands in its life, as the census counts it.
    ///
    /// The spellings are `core.fleets.status`'s, byte for byte, and the set
    /// mirrors the lifecycle crate's own status enum — that crate depends on
    /// this one, so the two cannot share a type, and a test over there holds
    /// them equal member for member instead.
    FleetStatusLabel {
        /// The row exists; its stream may not yet.
        Installing => "installing",
        /// Leasable. The only status the runner's candidate query admits.
        Active => "active",
        /// Held by the platform's anomaly gate.
        Paused => "paused",
        /// Stopped by an operator, and resumable.
        Stopped => "stopped",
        /// Terminal.
        Killed => "killed",
    }
}

impl FleetStatusLabel {
    /// The member a stored spelling names, if this build models it.
    ///
    /// `None` rather than a default: a row holding a status this build does
    /// not know is dropped from the census and reported, never counted under
    /// a member it is not.
    #[must_use]
    pub fn from_spelling(raw: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|status| status.as_str() == raw)
    }
}

closed_set! {
    /// How a run came to start.
    ///
    /// Two rather than two families: an operator reads them as one line split
    /// by cause, and `sum()` over one family stays the count of runs begun.
    RunStart {
        /// A new entry read off the stream.
        Fresh => "fresh",
        /// A lapsed holder's event, re-leased under a higher fence.
        Reclaimed => "reclaimed",
    }
}
