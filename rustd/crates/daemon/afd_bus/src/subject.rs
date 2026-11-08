use af_core::id::Uuid7;

/// Signal addresses; each adapter owns their physical spelling.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// Cosmetic live activity for one fleet.
    FleetTail(Uuid7),
    /// Authoring model progress for one draft.
    DraftEvents(Uuid7),
    /// A recent-work notification on one of the fixed shards.
    Wake(u16),
    /// Recheck or cancellation for one runner host.
    RunnerCommand(Uuid7),
    /// An owed outbound answer was committed; a worker should claim now
    /// rather than at its next pass (M244 §4).
    OutboundWake,
}

impl Subject {
    /// Bounded spelling for telemetry labels; identifiers never become labels.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::FleetTail(_) => "fleet_tail",
            Self::DraftEvents(_) => "draft_events",
            Self::Wake(_) => "wake",
            Self::RunnerCommand(_) => "runner_command",
            Self::OutboundWake => "outbound_wake",
        }
    }
}
