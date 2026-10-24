//! Native platform publication has no repository, file or bundle input.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Stage an authorized creator-owned current candidate, initially hidden.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Workspace containing the authenticated person's private draft.
    pub workspace_id: String,
    /// Exact current candidate, resolved under creator and workspace authority.
    pub draft_candidate_id: String,
    /// Human display label; the stable slug comes from validated setup.
    pub label: String,
}

/// Conditional platform-owned metadata edit.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "openapi", schema(as = PlatformLibraryPatch))]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// Display label.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    pub name: Option<String>,
    /// Curated description.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    pub description: Option<String>,
    /// Credential-name explanations, never credential values.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    pub required_credentials_reasons: Option<BTreeMap<String, String>>,
    /// Explicitly publish or withdraw the staged entry.
    #[serde(default, deserialize_with = "crate::authoring::present")]
    pub published: Option<bool>,
}

/// Platform metadata without instructions or private authoring content.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct Item<R = serde_json::Value> {
    /// Immutable head metadata with the current resource tag.
    pub card: crate::library::Card<R>,
    /// Whether new tenant discovery and installation are allowed.
    pub published: bool,
    /// Last metadata update timestamp, such as `2026-10-24T05:00:00.000Z`.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: jiff::Timestamp,
}

/// Staged and published platform metadata.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Serialize)]
pub struct List<R = serde_json::Value> {
    /// Native platform entries.
    pub entries: Vec<Item<R>>,
}
