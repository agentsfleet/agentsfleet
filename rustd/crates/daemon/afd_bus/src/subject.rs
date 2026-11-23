use af_core::id::Uuid7;

/// Signal addresses; each adapter owns their physical spelling.
///
/// A thread is a fleet or a setup draft, named by its own id.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// A queued turn is waiting on this thread. Every replica reads every
    /// thread's wake through one wildcard subscription.
    ThreadWake(Uuid7),
    /// Live items and frames for one thread's open streams.
    ThreadItems(Uuid7),
    /// Recheck or cancellation for one runner host.
    RunnerCommand(Uuid7),
    /// An owed delivery was committed; a worker should claim now rather than
    /// at its next pass. Lossy: a missed ping costs one pass.
    DeliveryWake,
    /// Frames on any subject may be missing; every reader re-reads Postgres.
    Gap,
}

impl Subject {
    /// Bounded spelling for telemetry labels; identifiers never become labels.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ThreadWake(_) => "thread_wake",
            Self::ThreadItems(_) => "thread_items",
            Self::RunnerCommand(_) => "runner_command",
            Self::DeliveryWake => "delivery_wake",
            Self::Gap => "gap",
        }
    }
}
