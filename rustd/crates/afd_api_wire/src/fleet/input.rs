//! File mutation cannot bypass private authoring and human review.

use serde::{Deserialize, Serialize};

/// Display-only provenance; it grants no authority or access.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SaveOrigin {
    /// Native command-line client.
    Cli,
    /// Dashboard setup controls.
    Dashboard,
    /// A completed setup-chat save turn.
    Chat,
}

/// One immutable source accepted by Save.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SaveSource {
    /// Current creator-owned idle candidate.
    Draft {
        /// Copy `candidate.id` from `GET /v1/workspaces/{workspace_id}/drafts/{draft_id}`.
        draft_candidate_id: String,
    },
    /// Pinned accessible Library revision.
    Library {
        /// Entry identity.
        library_id: String,
        /// Immutable revision identity.
        revision_id: String,
    },
}

/// Saving and running are distinct explicit choices.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Start {
    /// Create without a runtime event.
    None,
    /// Queue this message once after activation.
    RunNow {
        /// Nonempty text, at most 32,768 UTF-8 bytes.
        message: String,
    },
}

/// POST fleets, with one required header operation key.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveFleetRequest {
    /// Optional client-reported save surface, used only in the activity notice.
    pub origin: Option<SaveOrigin>,
    /// Exact source that was reviewed.
    pub source: SaveSource,
    /// Validated instance slug.
    pub name: String,
    /// Copy `review.review_digest` from the source review after inspecting it.
    ///
    /// For a draft, read `GET /v1/workspaces/{workspace_id}/drafts/{draft_id}`. Reload the
    /// review before saving if it changes.
    pub review_digest: String,
    /// Explicit initial-run choice.
    pub start: Start,
}

/// PATCH fleets changes lifecycle or name under its required resource tag.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct PatchFleetRequest {
    /// Supported lifecycle target, omitted to keep current state.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    #[garde(skip)]
    pub status: Option<PatchStatus>,
    /// Validated instance slug, omitted to retain it.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    #[garde(inner(length(bytes, min = 1, max = 64)))]
    pub name: Option<String>,
}

/// Human-requestable lifecycle targets exclude system-owned pause states.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchStatus {
    /// Activate or resume checked setup.
    Active,
    /// Stop an activated fleet.
    Stopped,
    /// Kill the fleet or cancel its unfinished setup.
    Killed,
}

impl PatchStatus {
    /// Existing stored lifecycle vocabulary.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Stopped => "stopped",
            Self::Killed => "killed",
        }
    }
}
