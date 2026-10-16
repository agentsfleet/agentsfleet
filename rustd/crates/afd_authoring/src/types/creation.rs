//! Save has one retained fleet identity and an explicit initial-run choice.

use super::{MAX_MESSAGE_BYTES, MAX_MODEL_REF_BYTES};
use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

/// Immutable setup selected by a reviewed Save.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SaveSource {
    /// The creator's current, idle native candidate.
    Draft {
        /// Accepted immutable candidate.
        #[garde(skip)]
        draft_candidate_id: Uuid7,
    },
    /// Exactly this accessible Library revision.
    Library {
        /// Selected opaque entry identifier.
        #[garde(
            length(bytes, min = 1, max = 128),
            custom(afd_validate::rules::nul_free)
        )]
        library_id: String,
        /// Selected immutable revision.
        #[garde(skip)]
        revision_id: Uuid7,
    },
}

/// Save alone never implies a runtime message.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Start {
    /// Create without running now.
    None,
    /// One message, bound to the daemon-derived creation identity.
    RunNow {
        /// Ordinary shared runtime input.
        #[garde(length(bytes, min = 1, max = MAX_MESSAGE_BYTES), custom(afd_validate::rules::nul_free))]
        message: String,
    },
}

/// Reviewed source and choices; replay identity is the single header key.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct SaveFleet {
    /// Candidate or pinned Library source.
    #[garde(dive)]
    pub source: SaveSource,
    /// Validated instance slug, independent of the recipe name.
    #[garde(length(bytes, min = 1, max = 64))]
    pub name: String,
    /// Digest returned by a fresh human review.
    #[garde(length(bytes, min = 1, max = MAX_MODEL_REF_BYTES))]
    pub review_digest: String,
    /// Explicit Save or Save and run now choice.
    #[garde(dive)]
    pub start: Start,
}

/// Public initial-run progress, independent of activation and completed work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartState {
    /// Durable admission or its receipt is not yet confirmed.
    Pending,
    /// The admission has its saved queue receipt.
    Accepted,
    /// Capacity refused before a durable admission existed.
    Blocked,
}

/// Public initial-run progress, independent of activation and completed work.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StartOutcome {
    /// No initial event was requested.
    None,
    /// The same initial event remains pending, accepted or blocked.
    RunNow {
        /// `pending`, `accepted` or `blocked`.
        state: StartState,
        /// Present once a durable admission exists.
        #[serde(skip_serializing_if = "Option::is_none")]
        event_id: Option<String>,
        /// Stable public refusal code when blocked.
        #[serde(skip_serializing_if = "Option::is_none")]
        error_code: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct InitialStart {
    pub(crate) choice: Start,
    pub(crate) outcome: StartOutcome,
}

impl InitialStart {
    pub(crate) fn new(choice: Start) -> Self {
        let outcome = match &choice {
            Start::None => StartOutcome::None,
            Start::RunNow { .. } => StartOutcome::RunNow {
                state: StartState::Pending,
                event_id: None,
                error_code: None,
            },
        };
        Self { choice, outcome }
    }

    pub(crate) fn unadmitted(&self) -> bool {
        matches!(self.outcome, StartOutcome::RunNow { event_id: None, .. })
    }
}
