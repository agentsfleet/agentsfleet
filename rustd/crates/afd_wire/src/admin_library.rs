//! Wire shapes for platform Fleet-library and bundle administration.
//!
//! Split from [`admin_catalogue`](crate::admin_catalogue), which names the
//! priced-model half. Both reach callers through [`crate::admin`], so the two
//! files are a reading convenience and not a boundary any consumer sees.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Platform Fleet-library onboarding request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AdminLibraryImport<'a> {
    /// `upload`, `github`, or first-party `template`.
    #[serde(borrow)]
    pub source_kind: Cow<'a, str>,
    /// Repository, template id, or upload provenance.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// Optional GitHub branch, tag, or commit.
    #[serde(borrow, rename = "ref")]
    pub revision: Option<Cow<'a, str>>,
    /// Explicitly permits replacing a slug owned by another source.
    pub replace: bool,
    /// Inline root document for uploads.
    #[serde(borrow)]
    pub skill_markdown: Option<Cow<'a, str>>,
    /// Optional inline trigger document for uploads.
    #[serde(borrow)]
    pub trigger_markdown: Option<Cow<'a, str>>,
    /// Attachments are fetched from repositories; inline uploads reject these.
    pub support_files: Vec<serde_json::Value>,
}

/// Content-free requirements shown on one Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryRequirements<'a> {
    /// Credential names only.
    #[serde(borrow)]
    pub credentials: Vec<Cow<'a, str>>,
    /// Required tool names.
    #[serde(borrow)]
    pub tools: Vec<Cow<'a, str>>,
    /// Declared outbound hosts.
    #[serde(borrow)]
    pub network_hosts: Vec<Cow<'a, str>>,
    /// Whether a trigger document exists.
    pub trigger_present: bool,
}

/// One metadata-only platform Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryItem<'a> {
    /// Slug identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Curated description.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// GitHub owner/repository.
    #[serde(borrow)]
    pub source_repo: Cow<'a, str>,
    /// Fetched revision.
    #[serde(borrow)]
    pub source_ref: Cow<'a, str>,
    /// Draft or public.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// Content identity, never support-file bytes.
    #[serde(borrow)]
    pub content_hash: Option<Cow<'a, str>>,
    /// Derived requirement names and trigger presence.
    #[serde(borrow)]
    pub requirements: AdminLibraryRequirements<'a>,
    /// Operator-authored per-credential reason copy.
    pub required_credentials_reasons: serde_json::Value,
    /// Last mutation instant in epoch milliseconds.
    pub updated_at: i64,
    /// Strong version over the editable row surface.
    #[serde(borrow)]
    pub etag: Cow<'a, str>,
}

/// Admin Fleet-library list response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibrariesResponse<'a> {
    /// Every draft and public row.
    #[serde(borrow)]
    pub entries: Vec<AdminLibraryItem<'a>>,
}

/// Successful platform Fleet-library onboarding.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryCreated<'a> {
    /// Slug derived from `SKILL.md`.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name derived from `SKILL.md`.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Which library the entry now stands in.
    ///
    /// `platform` from the operator's catalogue and `tenant` from a workspace
    /// onboard — both verbs answer this shape, and the tier is what differs.
    #[serde(borrow)]
    pub visibility: Cow<'a, str>,
    /// Content identity of the validated bundle.
    #[serde(borrow)]
    pub content_hash: Cow<'a, str>,
    /// Credential/tool/host names without support-file paths.
    #[serde(borrow)]
    pub requirements: AdminLibraryRequirements<'a>,
}

/// One public Fleet Bundle gallery row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetBundleItem<'a> {
    /// Stable catalogue slug.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Curated summary.
    #[serde(borrow)]
    pub description: Cow<'a, str>,
    /// Credential names, never values.
    #[serde(borrow)]
    pub required_credentials: Vec<Cow<'a, str>>,
    /// Install-gate explanation keyed by credential name.
    pub required_credentials_reasons: serde_json::Value,
    /// Required tool identifiers.
    #[serde(borrow)]
    pub required_tools: Vec<Cow<'a, str>>,
    /// Declared outbound hosts.
    #[serde(borrow)]
    pub network_hosts: Vec<Cow<'a, str>>,
}

/// Public Fleet Bundle gallery response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetBundlesResponse<'a> {
    /// Every published row carrying current bundle content.
    #[serde(borrow)]
    pub items: Vec<FleetBundleItem<'a>>,
}

/// Partial operator edit for one Fleet-library row.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AdminLibraryPatch<'a> {
    /// Replacement display name.
    #[serde(borrow)]
    pub name: Option<Cow<'a, str>>,
    /// Replacement description.
    #[serde(borrow)]
    pub description: Option<Cow<'a, str>>,
    /// Replacement GitHub owner/repository.
    #[serde(borrow)]
    pub source_repo: Option<Cow<'a, str>>,
    /// Replacement branch or tag.
    #[serde(borrow)]
    pub source_ref: Option<Cow<'a, str>>,
    /// Operator-authored reason copy.
    pub required_credentials_reasons: Option<serde_json::Value>,
    /// Publish or withdraw.
    pub published: Option<bool>,
}
