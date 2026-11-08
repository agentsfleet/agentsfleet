//! Revision application carries no implicit Library publication or runtime message.

use crate::schedule::View;
use serde::{Deserialize, Serialize};

/// One current idle improvement candidate and its contextual human review.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRevision {
    /// Optional client-reported save surface, used only in the activity notice.
    pub origin: Option<super::SaveOrigin>,
    /// Creator-owned immutable candidate `UUIDv7`.
    pub draft_candidate_id: String,
    /// Fresh contextual review digest.
    pub review_digest: String,
}

/// Accepted setup identity remains stable when its operation is replayed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct AppliedRevision<'a> {
    /// Target fleet `UUIDv7`.
    pub fleet_id: String,
    /// Decimal accepted setup revision.
    pub setup_revision: String,
    /// Strong accepted setup tag.
    pub setup_etag: String,
    /// Current fleet name/status tag.
    pub etag: String,
    /// Human review accepted for this operation.
    pub review_digest: String,
    /// Actual current inherited and independent schedule observations.
    pub schedules: Vec<View<'a>>,
}
