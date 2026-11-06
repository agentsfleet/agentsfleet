//! Native lifecycle vocabulary shared by daemon and runner.

use serde::{Deserialize, Serialize};

/// The durable objective's state, independent of its current admission.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskState {
    /// Eligible for one existing admission/lease attempt.
    Ready,
    /// A current lease is advancing this objective.
    Running,
    /// An external dependency is recorded and compute has been released.
    Waiting,
    /// Progress needs a named prerequisite or clarification.
    Blocked,
    /// A person suspended new work; elapsed time and usage are retained.
    Paused,
    /// Current native evidence satisfied the accepted objective.
    Done,
    /// An authorized stop fenced subsequent effects.
    Cancelled,
    /// A cumulative allowance or the wall deadline was exhausted.
    Exhausted,
}

impl TaskState {
    /// Whether a later event is forbidden from reopening this owner.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled | Self::Exhausted)
    }

    /// The stored spelling, kept equal to the shared serde vocabulary.
    ///
    /// This match intentionally has no fallback; a new state needs an explicit
    /// storage spelling and lifecycle review before it can compile.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Blocked => "blocked",
            Self::Paused => "paused",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
            Self::Exhausted => "exhausted",
        }
    }
}
