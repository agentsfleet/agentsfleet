//! Wire shapes for platform-key and priced-model administration.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

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
