//! Live-tail publish and shared outbound-stream value types.

pub(crate) mod retain;
mod tail;
pub use self::retain::{ACKNOWLEDGED_HISTORY, Backlog, TRIM_SLACK, Trimmed};
pub(crate) use self::tail::CMD_PUBLISH;
use crate::Dragonfly;

pub(crate) const ARG_COUNT: &str = "COUNT";
pub(crate) const CMD_XRANGE: &str = "XRANGE";

/// The sharded live-tail channel for a fleet.
#[must_use]
pub fn fleet_activity_channel(fleet: &str) -> String {
    format!("fleet:{fleet}:activity")
}

/// A Dragonfly stream entry id: the receipt an append answers with, and the only
/// thing `XACK` and a claim accept.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

impl EventId {
    /// The receipt as Dragonfly spelled it, `{millis}-{sequence}`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A receipt read back out of a column rather than minted by an append.
    ///
    /// The report path needs this: the entry was acknowledged long after the
    /// poll that read it, by a different request, and what survives in between
    /// is the `fleet.obligations.receipt` text. Deliberately not a `From`
    /// impl — a receipt is a thing Dragonfly produced, and a blanket conversion
    /// from `&str` would let any string in the program become one silently.
    #[must_use]
    pub fn of(stored: &str) -> Self {
        Self(stored.to_owned())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Publish operations over the shared Dragonfly connection.
#[derive(Debug, Clone)]
pub struct FleetStreams {
    redis: Dragonfly,
}
impl FleetStreams {
    /// Bind the publisher to an existing connection.
    #[must_use]
    pub const fn new(redis: Dragonfly) -> Self {
        Self { redis }
    }
}
