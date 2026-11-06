//! Wire shapes for platform-key and priced-model administration.

use std::borrow::Cow;

use afd_wire::decision::DecisionModelSettings;
use afd_wire::inference::ModelInference;
use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::catalogue::{CatalogueMetadata, CataloguePrice, PRICES_MAX, price_schedule};

/// `PUT /v1/admin/platform-keys` metadata; key bytes already live in the vault.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeyPut<'a> {
    /// Provider and vault-row name.
    #[serde(borrow)]
    #[garde(
        length(bytes, min = 1, max = KEY_PROVIDER_MAX_BYTES),
        custom(afd_validate::rules::nul_free)
    )]
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
    #[garde(
        length(bytes, min = 1, max = MODEL_ID_MAX_BYTES),
        custom(afd_validate::rules::nul_free)
    )]
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
    /// Last mutation instant.
    #[serde(with = "afd_wire::instant")]
    pub updated_at: jiff::Timestamp,
}

/// `GET /v1/admin/platform-keys` response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlatformKeysResponse<'a> {
    /// Every active and inactive provider row.
    #[serde(borrow)]
    pub keys: Vec<PlatformKeyItem<'a>>,
    /// Reference identifier to include when contacting support.
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
    /// Reference identifier to include when contacting support.
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
    /// Reference identifier to include when contacting support.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
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

/// The longest provider identity the catalogue stores.
pub const PROVIDER_MAX_BYTES: usize = 64;

/// The longest provider-native model identity the catalogue stores.
pub const MODEL_ID_MAX_BYTES: usize = afd_wire::inference::MODEL_ID_MAX_BYTES;

/// Decision settings and their protocol-specific complete price schedule.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AdminDecisionModel<'a> {
    /// Activation is independent of the immutable reviewed definition.
    #[garde(skip)]
    pub enabled: bool,
    /// Responses borrow the stored definition; request decoding owns its fields.
    #[serde(borrow)]
    #[garde(dive)]
    pub profile: Cow<'a, DecisionModelSettings>,
    /// The schedule belongs to this decision revision, independently of coding.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(min_items = 1, max_items = 64))]
    #[garde(length(min = 1, max = PRICES_MAX), dive, custom(price_schedule))]
    pub prices: Vec<CataloguePrice<'a>>,
}

/// `POST /v1/admin/models` input: one identity, its evidence, its complete
/// price schedule and, optionally, its inference settings.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AdminModelCreate<'a> {
    /// Provider identity.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = PROVIDER_MAX_BYTES), custom(afd_validate::rules::nul_free))]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MODEL_ID_MAX_BYTES), custom(afd_validate::rules::nul_free))]
    pub model_id: Cow<'a, str>,
    /// Label and context evidence.
    #[serde(borrow)]
    #[garde(dive)]
    pub metadata: CatalogueMetadata<'a>,
    /// Every prompt tier across every date range, in integer nanos per
    /// million tokens.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(max_items = 64))]
    #[garde(length(max = PRICES_MAX), dive, custom(coding_schedule(self.inference.is_none() && self.decision.is_some())))]
    pub prices: Vec<CataloguePrice<'a>>,
    /// Explicit inference settings; omitted rows remain unavailable for inference.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub inference: Option<ModelInference>,
    /// Omission leaves a new model without decision capability.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub decision: Option<AdminDecisionModel<'a>>,
}

/// `PATCH /v1/admin/models/{id}` input: a complete replacement of the
/// evidence and the price schedule, and optionally of the inference settings.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct AdminModelPatch<'a> {
    /// Label and context evidence.
    #[serde(borrow)]
    #[garde(dive)]
    pub metadata: CatalogueMetadata<'a>,
    /// The whole coding schedule; the saved settings determine whether it can
    /// be empty. A range absent here is removed.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(max_items = 64))]
    #[garde(length(max = PRICES_MAX), dive, custom(coding_schedule(true)))]
    pub prices: Vec<CataloguePrice<'a>>,
    /// Omission preserves the current inference settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub inference: Option<ModelInference>,
    /// Omission preserves the saved decision definition and schedule.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    #[garde(dive)]
    pub decision: Option<AdminDecisionModel<'a>>,
}

/// One model in the admin list.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelItem<'a> {
    /// Model record's version 7 universally unique identifier (UUID).
    #[serde(borrow)]
    pub id: Cow<'a, str>,
    /// Provider identity.
    #[serde(borrow)]
    pub provider: Cow<'a, str>,
    /// Provider-native model identity.
    #[serde(borrow)]
    pub model_id: Cow<'a, str>,
    /// Label and context evidence.
    #[serde(borrow)]
    pub metadata: CatalogueMetadata<'a>,
    /// The complete schedule, ordered by tier then start.
    #[serde(borrow)]
    pub prices: Vec<CataloguePrice<'a>>,
    /// Non-secret settings used by setup chat and runtime leases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inference: Option<ModelInference>,
    /// Reviewed decision definition and independent rates, when configured.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<AdminDecisionModel<'a>>,
}

/// `GET /v1/admin/models` response.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminModelsResponse<'a> {
    /// One bounded page of priced rows.
    #[serde(borrow)]
    pub models: Vec<AdminModelItem<'a>>,
    /// Continue with starting_after, or null at the end of this catalogue snapshot.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
    /// Reference identifier to include when contacting support.
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
    /// Reference identifier to include when contacting support.
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
    /// Reference identifier to include when contacting support.
    #[serde(borrow)]
    pub request_id: Cow<'a, str>,
}

fn coding_schedule<C: ?Sized>(
    decision_only: bool,
) -> impl Fn(&[CataloguePrice<'_>], &C) -> garde::Result {
    move |prices, context| {
        if decision_only && prices.is_empty() {
            Ok(())
        } else {
            price_schedule(prices, context)
        }
    }
}
