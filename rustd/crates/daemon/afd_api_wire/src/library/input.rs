//! File, repository and template sources have no publication input.

use serde::Deserialize;

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Explicitly publish a creator-owned chat candidate as a new reusable entry.
pub struct Publish {
    /// Current creator-private idle candidate `UUIDv7`.
    pub draft_candidate_id: String,
    /// Display label, 1..100 characters.
    pub label: String,
}

#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Explicitly append a chat candidate to the selected owned entry.
pub struct PublishRevision {
    /// Current creator-private idle candidate `UUIDv7`.
    pub draft_candidate_id: String,
}
