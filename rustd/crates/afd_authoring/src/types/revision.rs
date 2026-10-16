//! A reviewed candidate replaces one explicitly pinned fleet setup.

use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};

/// Applies a current idle improvement candidate to its original fleet.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ApplyRevision {
    /// Immutable candidate owned by the authenticated creator.
    #[garde(skip)]
    pub draft_candidate_id: Uuid7,
    /// Contextual review accepted by the person.
    #[garde(length(bytes,min=1,max=super::MAX_MODEL_REF_BYTES))]
    pub review_digest: String,
}

/// Accepted setup identity plus the fleet's current resource tag.
#[derive(Debug, Serialize)]
pub struct AppliedRevision {
    /// Target shared runtime identity.
    pub fleet_id: Uuid7,
    /// Decimal revision accepted by this operation, stable on replay.
    pub setup_revision: String,
    /// Strong tag of the accepted immutable setup.
    pub setup_etag: String,
    /// Current name/status resource tag.
    pub etag: String,
    /// Human review accepted by this operation.
    pub review_digest: String,
}
