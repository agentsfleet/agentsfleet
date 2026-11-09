//! Finite operation names shared by human errors, metrics and traces.

/// The session operation that failed; never derived from a payload or identifier.
#[derive(Debug, Clone, Copy)]
pub enum Operation {
    /// Read and confirm connection ownership.
    Opening,
    /// Place one new lease.
    Offer,
    /// Reconcile liveness and assigned policy.
    Heartbeat,
    /// Refresh authority after a broker command or gap.
    Recheck,
    /// Extend one lease and acknowledge its usage.
    Renewal,
    /// Await one already-accounted physical authoring provider invocation.
    Model,
    /// Publish bounded progress.
    Activity,
    /// Settle a durable terminal report.
    Report,
    /// Read fenced pending inputs.
    Folds,
    /// Write and flush a complete frame.
    Write,
    /// Finish accepted requests and close the session.
    Drain,
}

impl Operation {
    /// Stable spelling for a bounded observation label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Offer => "offer",
            Self::Heartbeat => "heartbeat",
            Self::Recheck => "recheck",
            Self::Renewal => "renewal",
            Self::Activity => "activity",
            Self::Report => "report",
            Self::Folds => "folds",
            Self::Model => "model",
            Self::Write => "write",
            Self::Drain => "drain",
        }
    }
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Opening => "confirming the runner connection",
            Self::Offer => "offering work",
            Self::Heartbeat => "recording a heartbeat",
            Self::Recheck => "checking the current owner",
            Self::Renewal => "renewing work",
            Self::Activity => "publishing progress",
            Self::Report => "settling a report",
            Self::Folds => "reading pending inputs",
            Self::Write => "sending a response",
            Self::Model => "calling the authoring model",
            Self::Drain => "closing the runner session",
        })
    }
}
