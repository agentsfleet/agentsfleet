//! One session protocol over the runner's HTTP/3 and HTTP/2 carriers.

mod down;
mod up;

pub use down::{Answer, Down, RenewalAnswer};
pub use up::{Renewal, Up};

/// The newest revision, negotiated before the server accepts a runner session.
///
/// It adds draft-owned runs on the common run path: a v3 runner may be
/// assigned a [`crate::run::RunPayload`] whose `owner` names a setup draft.
/// The daemon accepts every [`Revision`] and answers the one the runner sent.
pub const VERSION: &str = "agentsfleet.runner.v3";
/// The revision before setup rode the common run path: its runner is never
/// assigned a draft-owned run, and keeps the separate authoring run.
pub const VERSION_V2: &str = "agentsfleet.runner.v2";

/// One protocol revision a daemon accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Revision {
    /// [`VERSION_V2`].
    V2,
    /// [`VERSION`].
    V3,
}

impl Revision {
    /// The header spelling of this revision.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V2 => VERSION_V2,
            Self::V3 => VERSION,
        }
    }

    /// The revision `raw` names, or `None` for one this build does not speak.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            VERSION_V2 => Some(Self::V2),
            VERSION => Some(Self::V3),
            _ => None,
        }
    }

    /// Whether a runner on this revision may be assigned a draft-owned run.
    #[must_use]
    pub const fn assigns_draft_runs(self) -> bool {
        matches!(self, Self::V3)
    }
}
/// Application version; independent of the negotiated HTTP version.
pub const VERSION_HEADER: &str = "agentsfleet-runner-protocol";
/// Which independently flow-controlled request this stream serves.
pub const STREAM_KIND_HEADER: &str = "agentsfleet-runner-kind";
/// Previous owner on a reconnecting control stream; current owner on secondary streams.
pub const CONNECTION_HEADER: &str = "agentsfleet-runner-connection";
/// Maximum JSON payload, excluding its four-byte length prefix.
pub const MESSAGE_BYTES_MAX: usize = 2 * 1024 * 1024;
/// The host worker bound also limits running runs and renewal entries.
pub const RUNS_MAX: usize = crate::runner::HOLDS_MAX;
/// Pending request identifiers retained by one session.
pub const PENDING_MAX: usize = 256;

/// Independent HTTP request streams within one authenticated connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StreamKind {
    /// Assignments, cancellation, renewals and small request/answer exchanges.
    Control,
    /// Assignments and folded inputs, independently flow-controlled from control.
    Work,
    /// Durable terminal reports and their acknowledgements.
    Reports,
    /// Bounded, expendable live progress.
    Activity,
}

impl StreamKind {
    /// Every kind required before the session can accept work.
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

    /// Refuses an unsupported kind without substituting a default.
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
