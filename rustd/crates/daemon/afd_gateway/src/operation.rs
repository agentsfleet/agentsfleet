//! Finite operation names shared by human errors, metrics and traces.

/// The connection operation that failed; never derived from a payload or identifier.
#[derive(Debug, Clone, Copy)]
pub enum Operation {
    /// Read and confirm connection ownership.
    Opening,
    /// Place one new run.
    Assign,
    /// Reconcile liveness and assigned policy.
    Heartbeat,
    /// Refresh authority after a broker command or gap.
    Recheck,
    /// Extend one run and acknowledge its usage.
    Renewal,
    /// Await one already-accounted physical authoring provider invocation.
    Model,
    /// Publish bounded progress.
    Progress,
    /// Settle a durable terminal report.
    Report,
    /// Read fenced pending inputs.
    Folds,
    /// Capture or restore ordinary event-owned recovery state.
    TurnCheckpoint,
    /// Read or save a setup run's compacted draft context.
    Context,
    /// Write and flush a complete frame.
    Write,
    /// Finish accepted requests and close the connection.
    Drain,
}

impl Operation {
    /// Stable spelling for a bounded observation label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Opening => "opening",
            Self::Assign => "assign",
            Self::Heartbeat => "heartbeat",
            Self::Recheck => "recheck",
            Self::Renewal => "renewal",
            Self::Progress => "progress",
            Self::Report => "report",
            Self::Folds => "folds",
            Self::TurnCheckpoint => "turn_checkpoint",
            Self::Context => "context",
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
            Self::Assign => "assigning work",
            Self::Heartbeat => "recording a heartbeat",
            Self::Recheck => "checking the current owner",
            Self::Renewal => "renewing work",
            Self::Progress => "publishing progress",
            Self::Report => "settling a report",
            Self::Folds => "reading pending inputs",
            Self::TurnCheckpoint => "saving or restoring a turn checkpoint",
            Self::Context => "reading or saving a setup context",
            Self::Write => "sending a response",
            Self::Model => "calling the authoring model",
            Self::Drain => "closing the runner connection",
        })
    }
}
