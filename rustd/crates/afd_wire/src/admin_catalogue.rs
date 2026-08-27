//! Wire shapes for platform-key and priced-model administration.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Platform Fleet-library onboarding request.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibrariesResponse<'a> {
    /// Every draft and public row.
    #[serde(borrow)]
    pub entries: Vec<AdminLibraryItem<'a>>,
}

/// Successful platform Fleet-library onboarding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminLibraryCreated<'a> {
    /// Slug derived from `SKILL.md`.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Display name derived from `SKILL.md`.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// Always `platform` for this endpoint.
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetBundlesResponse<'a> {
    /// Every published row carrying current bundle content.
    #[serde(borrow)]
    pub items: Vec<FleetBundleItem<'a>>,
}

/// Partial operator edit for one Fleet-library row.
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

/// `PUT /v1/admin/platform-keys` metadata; key bytes already live in the vault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyPut<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Workspace holding that vault row.
    #[serde(borrow)]
    pub source_workspace_id: Cow<'a, str>,
    /// Priced model selected as platform default.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// Custom endpoint for the compatible-provider mode.
    #[serde(borrow)]
    pub base_url: Option<Cow<'a, str>>,
}

/// Reveal-free platform-key list item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyItem<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Workspace holding the key.
    #[serde(borrow)]
    pub source_workspace_id: Cow<'a, str>,
    /// Active priced model, absent after deactivation.
    #[serde(borrow)]
    pub model: Option<Cow<'a, str>>,
    /// Whether this row is the platform default.
    pub active: bool,
    /// Last mutation instant in epoch milliseconds.
    pub updated_at: i64,
}

/// `GET /v1/admin/platform-keys` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeysResponse<'a> {
    /// Every active and inactive provider row.
    #[serde(borrow)]
    pub keys: Vec<PlatformKeyItem<'a>>,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

/// Successful platform-default activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeySetResponse<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Selected priced model.
    #[serde(borrow)]
    pub model: Cow<'a, str>,
    /// Workspace holding the key.
    #[serde(borrow)]
    pub source_workspace_id: Cow<'a, str>,
    /// Always true after activation.
    pub active: bool,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

/// Successful platform-default deactivation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyDeactivateResponse<'a> {
    /// Provider that was deactivated.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Always false after deactivation.
    pub active: bool,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

/// Mutable rate fields shared by admin model create and patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelRates {
    /// Maximum context tokens.
    pub context_cap_tokens: i32,
    /// Input-token nanos per million tokens.
    pub input_nanos_per_mtok: i64,
    /// Cached-input nanos per million tokens.
    pub cached_input_nanos_per_mtok: i64,
    /// Output-token nanos per million tokens.
    pub output_nanos_per_mtok: i64,
}

/// `POST /v1/admin/models` input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelCreate<'a> {
    /// Provider identity.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    pub model_id: Cow<'a, str>,
    /// Rates and context cap flattened on the existing wire.
    #[serde(flatten)]
    pub rates: ModelRates,
}

/// One priced model row in the admin list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelItem<'a> {
    /// Opaque `UUIDv7` row identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Provider identity.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    pub model_id: Cow<'a, str>,
    /// Rates and context cap flattened on the existing wire.
    #[serde(flatten)]
    pub rates: ModelRates,
}

/// `GET /v1/admin/models` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelsResponse<'a> {
    /// Every priced row.
    #[serde(borrow)]
    pub models: Vec<AdminModelItem<'a>>,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

/// Successful model creation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelCreated<'a> {
    /// The created model fields, flattened on the existing wire.
    #[serde(flatten, borrow)]
    pub model: AdminModelItem<'a>,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

/// Successful model rate update.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelUpdated<'a> {
    /// Updated row identity.
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Always true after an update.
    pub updated: bool,
    /// Server-generated support correlation id.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}
