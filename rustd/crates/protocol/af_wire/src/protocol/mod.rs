//! One session protocol over the runner's HTTP/3 and HTTP/2 carriers.

mod down;
mod up;

pub use down::{Answer, Down, RenewalAnswer};
pub use up::{Renewal, Up};

/// The one revision this build speaks, negotiated before the server accepts
/// a runner session; a runner sending any other is refused at hello.
///
/// It runs setup on the common run path: a runner may be assigned a
/// [`crate::run::RunPayload`] whose `owner` names a setup draft, and it reads
/// and saves that draft's compacted context with [`Up::ContextLoad`] and
/// [`Up::ContextSave`].
pub const VERSION: &str = "agentsfleet.runner.v4";
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
    Progress,
}

impl StreamKind {
    /// Every kind required before the session can accept work.
    pub const ALL: [Self; 4] = [Self::Control, Self::Work, Self::Reports, Self::Progress];

    /// Header spelling, shared by both carriers and the server.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Control => "control",
            Self::Work => "work",
            Self::Reports => "reports",
            Self::Progress => "progress",
        }
    }

    /// Refuses an unsupported kind without substituting a default.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "control" => Some(Self::Control),
            "work" => Some(Self::Work),
            "reports" => Some(Self::Reports),
            "progress" => Some(Self::Progress),
            _ => None,
        }
    }
}
