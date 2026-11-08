//! One session protocol over the runner's HTTP/3 and HTTP/2 carriers.

mod down;
mod up;

pub use down::{Answer, Down, RenewalAnswer};
pub use up::{Renewal, Up};

/// Negotiated before the server accepts a runner session.
pub const VERSION: &str = "agentsfleet.runner.v1";
/// Application version; independent of the negotiated HTTP version.
pub const VERSION_HEADER: &str = "agentsfleet-runner-protocol";
/// Which independently flow-controlled request this stream serves.
pub const LANE_HEADER: &str = "agentsfleet-runner-lane";
/// Previous owner on a reconnecting control lane; current owner on secondary lanes.
pub const SESSION_HEADER: &str = "agentsfleet-runner-session";
/// Maximum JSON payload, excluding its four-byte length prefix.
pub const MESSAGE_BYTES_MAX: usize = 2 * 1024 * 1024;
/// The host worker bound also limits running leases and renewal entries.
pub const LEASES_MAX: usize = crate::runner::HOLDS_MAX;
/// Pending request identifiers retained by one session.
pub const PENDING_MAX: usize = 256;

/// Independent HTTP request streams within one authenticated connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lane {
    /// Offers, cancellation, renewals and small request/answer exchanges.
    Control,
    /// Offers and folded inputs, independently flow-controlled from control.
    Work,
    /// Durable terminal reports and their acknowledgements.
    Reports,
    /// Bounded, expendable live progress.
    Activity,
}

impl Lane {
    /// Every lane required before the session can accept work.
    pub const ALL: [Self; 4] = [Self::Control, Self::Work, Self::Reports, Self::Activity];

    /// Header spelling, shared by both carriers and the server.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Work => "work",
            Self::Reports => "reports",
            Self::Activity => "activity",
        }
    }

    /// Refuses an unsupported lane without substituting a default.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "control" => Some(Self::Control),
            "work" => Some(Self::Work),
            "reports" => Some(Self::Reports),
            "activity" => Some(Self::Activity),
            _ => None,
        }
    }
}
