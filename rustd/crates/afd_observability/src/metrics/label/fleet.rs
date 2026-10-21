//! What the account, admission and runner paths label their outcomes with.

use crate::metrics::label::closed_set;

/// This one has been seen before, and the first delivery's answer stands.
///
/// One spelling for one fact, shared by the sets that observe it. Named rather
/// than written twice so they cannot be renamed apart while still meaning the
/// same thing.
const REPLAYED: &str = "replayed";

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
    /// Five, and the split is the point: the first four are the delivery being
    /// wrong and the last is this daemon being unable to act on a delivery that
    /// was right. An operator seeing a spike needs to know which half, because
    /// only one of them is theirs to fix.
    SignupFailure {
        /// The signature did not verify.
        BadSignature => "bad_sig",
        /// The delivery's timestamp is outside the replay window.
        StaleTimestamp => "stale_ts",
        /// The payload carried no address to open an account against.
        MissingEmail => "missing_email",
        /// The primary address was not one the provider had verified.
        UnverifiedEmail => "unverified_email",
        /// The database refused the write.
        DatabaseError => "db_error",
    }
}

closed_set! {
    /// The result of admitting work to the durable queue.
    AdmissionOutcome {
        /// The ledger row committed.
        Accepted => "accepted",
        /// Seen before; the first admission's identity stands.
        Replayed => REPLAYED,
        /// The durable write failed and the producer must retry.
        Refused => "refused",
        /// A fleet or deployment budget is spent.
        OverBudget => "over_budget",
    }
}

closed_set! {
    /// Which cap cut a chat lease's earlier turns.
    HistoryCut {
        /// More finished turns than a lease carries; the oldest were left.
        Turns => "turns",
        /// A message or answer past its text cap was cut.
        Text => "text",
        /// The turns passed the byte budget; the oldest were dropped.
        Bytes => "bytes",
    }
}

closed_set! {
    /// Who won the claim on a fleet whose slot named a sandbox held live at
    /// the claim. A hold past its deadline is no hold, and counts nothing.
    HeldClaim {
        /// The runner holding the fleet's sandbox, which continues in it.
        /// Counted when its lease resumes the hold, so a reclaim, which
        /// starts clean, counts nothing.
        Holder => "holder",
        /// Another runner, once the holder fell silent or could no longer
        /// lease: degraded, or no longer active.
        OtherAfterLapse => "other_after_lapse",
    }
}

closed_set! {
    /// Where a fleet stands in its life, as the census counts it.
    ///
    /// The spellings are `fleet.fleets.status`'s, byte for byte, and the set
    /// mirrors the lifecycle crate's own status enum — that crate depends on
    /// this one, so the two cannot share a type, and a test over there holds
    /// them equal member for member instead.
    FleetStatusLabel {
        /// The row exists; setup checks have not activated the fleet.
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
    /// One measured boundary from event creation to the first streamed chunk.
    DeliveryStage {
        /// Producer event timestamp to durable lease issue.
        EventToLease => "event_to_lease",
        /// Lease issue to the first stream frame received by the daemon.
        LeaseToFirstChunk => "lease_to_first_chunk",
        /// Producer event timestamp to first stream frame received by the daemon.
        EventToFirstChunk => "event_to_first_chunk",
        /// Execution invocation to its first safe stream chunk, as the runner measured it.
        ZombieToFirstChunk => "zombie_to_first_chunk",
    }
}

closed_set! {
    /// How a run came to start.
    ///
    /// Two rather than two families: an operator reads them as one line split
    /// by cause, and `sum()` over one family stays the count of runs begun.
    RunStart {
        /// An admission claimed for the first time.
        Fresh => "fresh",
        /// A lapsed holder's event, re-leased under a higher fence.
        Reclaimed => "reclaimed",
    }
}

closed_set! {
    /// The lost notification still leaves work discoverable in Postgres.
    ReadyWake {
        /// A publish failed or exceeded its deadline.
        Ring => "ring",
        /// A replica lost or lagged notifications.
        Gap => "gap",
    }
}
