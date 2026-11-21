//! What the account, admission and runner paths label their outcomes with.

use crate::metrics::label::closed_set;

pub use super::run_cache::{CacheOutcome, RunCache};
pub use super::runner_run::RunnerRun;

closed_set! {
    /// Packet counters are cumulative; observers export only the delta.
    StreamPacket {
        /// Packets placed on the network.
        Sent => "sent",
        /// Packets the loss detector marked lost.
        Lost => "lost",
        /// Sent packets carrying retransmitted data.
        Retransmitted => "retransmitted"
    }
}

closed_set! {
    /// Process side observing its one native TLS connection.
    StreamPeer {
        /// Outbound runner connector.
        Client => "client",
        /// Private daemon acceptor.
        Server => "server"
    }
}

closed_set! {
    /// Public negotiated algorithm names; no certificate or key data.
    StreamKeyExchange {
        /// Required hybrid group.
        X25519MlKem768 => "x25519_mlkem768",
        /// Rejected conventional group.
        X25519 => "x25519",
        /// Rejected conventional group.
        P256 => "p256",
        /// Rejected conventional group.
        P384 => "p384",
        /// No completed or recognized negotiation.
        Other => "other"
    }
}

closed_set! {
    /// Result of the required TLS key-agreement policy.
    StreamTlsOutcome {
        /// The completed handshake selected the required hybrid group.
        Accepted => "accepted",
        /// The handshake failed or selected another group.
        Failed => "failed"
    }
}

closed_set! {
    /// Native protocol used for one connection attempt.
    StreamTransport {
        /// QUIC with verified TLS.
        Http3 => "http3",
        /// TCP with verified TLS.
        Http2 => "http2"
    }
}

closed_set! {
    /// Final outcome; dropping an unfinished future remains visible.
    StreamOutcome {
        /// The operation completed.
        Success => "success",
        /// The socket or name lookup failed.
        Network => "network",
        /// The absolute deadline expired.
        Timeout => "timeout",
        /// Explicit cancellation won.
        Cancelled => "cancelled",
        /// A message or protocol rule was violated.
        Protocol => "protocol",
        /// The peer or channel closed.
        Closed => "closed",
        /// A local worker failed.
        Worker => "worker",
        /// Certificate verification or TLS configuration failed.
        Tls => "tls",
        /// The peer did not support the protocol.
        Unsupported => "unsupported",
        /// The caller dropped the future before its result.
        Abandoned => "abandoned"
    }
}

closed_set! {
    /// Operation whose elapsed time or timeout is measured.
    StreamStage {
        /// Reserving frame memory.
        Memory => "memory",
        /// Receiving a partial frame.
        Read => "read",
        /// Sending a complete frame.
        Write => "write",
        /// Resolving and connecting a socket.
        Connect => "connect",
        /// Verifying the peer and starting HTTP.
        Handshake => "handshake",
        /// Waiting for response headers.
        Open => "open",
        /// Waiting for native workers to stop.
        Close => "close"
    }
}

closed_set! {
    /// Only these failures permit the same trust policy over HTTP/2.
    StreamFallback {
        /// The QUIC socket could not connect.
        Network => "network",
        /// QUIC did not answer before its deadline.
        Timeout => "timeout",
        /// QUIC closed before opening the session.
        Closed => "closed",
        /// The peer does not support this QUIC version.
        Unsupported => "unsupported"
    }
}

closed_set! {
    /// A wake queue stores bounded scheduling notifications; durable work stays in Postgres.
    WakeQueueOutcome {
        /// A new fleet entered the queue.
        Added => "added",
        /// The fleet already had a queued reminder.
        Duplicate => "duplicate",
        /// A new fleet replaced the oldest reminder.
        Evicted => "evicted"
    }
}

/// This one has been seen before, and the first delivery's answer stands.
///
/// One spelling for one fact, shared by the sets that observe it. Named rather
/// than written twice so they cannot be renamed apart while still meaning the
/// same thing.
const REPLAYED: &str = "replayed";

closed_set! {
    /// Why opening an account from a signup delivery did not happen.
    ///
    /// Six, and the split is the point: the first four are the delivery being
    /// wrong and the last two are this daemon being unable to act on a delivery
    /// that was right. An operator seeing a spike needs to know which half,
    /// because only one of them is theirs to fix.
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
        /// The account opened, but the identity provider did not take its
        /// tenant, so the person's next token carries none.
        WritebackFailed => "writeback_failed",
    }
}

closed_set! {
    /// The result of admitting work to the durable queue.
    IncomingTurnOutcome {
        /// The ledger row committed.
        Accepted => "accepted",
        /// Seen before; the first turn's identity stands.
        Replayed => REPLAYED,
        /// The durable write failed and the producer must retry.
        Refused => "refused",
        /// A fleet or deployment budget is spent.
        OverBudget => "over_budget",
    }
}

closed_set! {
    /// What cut a chat run's earlier turns.
    HistoryCut {
        /// The assignment would not fit one run frame; the oldest turns were
        /// dropped until it did.
        Frame => "frame",
    }
}

closed_set! {
    /// Who won the assignment on a fleet whose thread named a sandbox held live
    /// at the assignment. A hold past its deadline is no hold, and counts nothing.
    HeldAssignment {
        /// The runner holding the fleet's sandbox, which continues in it.
        /// Counted when its run resumes the hold, so a reassignment, which
        /// starts clean, counts nothing.
        Holder => "holder",
        /// Another runner, once the holder fell silent or could no longer
        /// run: degraded, or no longer active.
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
        /// Terminal: archived by an operator or by the platform gate.
        Archived => "archived",
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
    /// One measured boundary between event creation, the run, the first
    /// streamed chunk, the report and the event's completion.
    DeliveryStage {
        /// Producer event timestamp to durable run issue.
        EventToAssign => "event_to_assign",
        /// Run issue to the first stream frame received by the daemon.
        AssignToFirstChunk => "assign_to_first_chunk",
        /// Producer event timestamp to first stream frame received by the daemon.
        EventToFirstChunk => "event_to_first_chunk",
        /// Execution invocation to its first safe stream chunk, as the runner measured it.
        RunToFirstChunk => "run_to_first_chunk",
        /// Producer event timestamp to the `turn_complete` publish: the whole round trip.
        EventToComplete => "event_to_complete",
        /// Run issue to the report's arrival at the daemon.
        AssignToReport => "assign_to_report",
        /// The report's arrival to the `turn_complete` publish.
        ReportToComplete => "report_to_complete",
    }
}

closed_set! {
    /// How a run came to start.
    ///
    /// Two rather than two families: an operator reads them as one line split
    /// by cause, and `sum()` over one family stays the count of runs begun.
    RunStart {
        /// A turn assigned for the first time.
        Fresh => "fresh",
        /// A lapsed holder's event, re-held under a higher fence.
        Reassigned => "reassigned",
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

closed_set! {
    /// A successful report response either settled new work or acknowledged its retry.
    ReportAnswer {
        /// The report first settled its run.
        Settled => "settled",
        /// The run's existing report was acknowledged again.
        Repeated => "repeated"
    }
}

closed_set! {
    /// Where an accepted steer went.
    SteerPlaced {
        /// A run of its own.
        Started => "started",
        /// A live run took it before its next model call.
        Folded => "folded",
        /// It waits its turn.
        Queued => "queued"
    }
}

closed_set! {
    /// What became of a message a live run took from its fleet.
    FoldOutcome {
        /// An assignment took it for the run.
        Assigned => "assigned",
        /// The run's report answered it.
        Settled => "settled"
    }
}

closed_set! {
    /// Committed automatic quarantine transitions; no runner identifier label.
    CordonChange {
        /// Silence crossed the offline threshold.
        Cordoned => "cordoned",
        /// Continuous healthy beats proved recovery.
        Lifted => "lifted"
    }
}
