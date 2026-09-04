//! Wire shapes for platform-key and priced-model administration.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// `PUT /v1/admin/platform-keys` metadata; key bytes already live in the vault.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyPut<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = KEY_PROVIDER_MAX_BYTES))]
    pub provider: Cow<'a, str>,
    /// Workspace holding that vault row.
    #[serde(borrow)]
    // Unbounded here: it is PARSED as a `Uuid7` downstream, and a parse is a
    // stricter proof than any length — a bound would only refuse earlier with
    // a vaguer sentence.
    #[garde(skip)]
    pub source_workspace_id: Cow<'a, str>,
    /// Priced model selected as platform default.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MODEL_ID_MAX_BYTES))]
    pub model: Cow<'a, str>,
    /// Custom endpoint for the compatible-provider mode.
    #[serde(borrow)]
    // Its legality depends on the PROVIDER beside it — a custom endpoint is
    // required for one mode and refused for the others — so it is proven by
    // `validate_endpoint_pair`, which can see both, not by a bound that can
    // only see one.
    #[garde(skip)]
    pub base_url: Option<Cow<'a, str>>,
}

/// Reveal-free platform-key list item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ModelRates {
    /// Maximum context tokens.
    #[garde(range(min = CONTEXT_CAP_MIN))]
    pub context_cap_tokens: i32,
    /// Input-token nanos per million tokens.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub input_nanos_per_mtok: i64,
    /// Cached-input nanos per million tokens.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub cached_input_nanos_per_mtok: i64,
    /// Output-token nanos per million tokens.
    #[garde(range(min = RATE_NANOS_MIN))]
    pub output_nanos_per_mtok: i64,
}

/// The longest provider name a platform key may carry.
///
/// Shorter than the catalogue's [`PROVIDER_MAX_BYTES`] on purpose: this one is
/// also a VAULT ROW NAME, and the vault's key space is the tighter of the two.
pub const KEY_PROVIDER_MAX_BYTES: usize = 32;

/// The smallest usable context ceiling.
///
/// A model priced with a cap of zero can serve no request at all, so the row is
/// refused rather than stored and discovered later by a run that cannot start.
pub const CONTEXT_CAP_MIN: i32 = 1;

/// The floor a price may not go under.
///
/// Zero is legal — a free model is a real thing — and negative is not: a
/// negative rate credits a tenant for spending, which the ledger has no reading
/// for.
pub const RATE_NANOS_MIN: i64 = 0;

/// The longest provider identity the catalogue stores.
pub const PROVIDER_MAX_BYTES: usize = 64;

/// The longest provider-native model identity the catalogue stores.
pub const MODEL_ID_MAX_BYTES: usize = 256;

/// `POST /v1/admin/models` input.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AdminModelCreate<'a> {
    /// Provider identity.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = PROVIDER_MAX_BYTES))]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MODEL_ID_MAX_BYTES))]
    pub model_id: Cow<'a, str>,
    /// Rates and context cap flattened on the existing wire.
    #[serde(flatten)]
    #[garde(dive)]
    pub rates: ModelRates,
}

/// One priced model row in the admin list.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
