//! `/v1/tenants/me/provider` — whose key this tenant's runs dial with.
//!
//! All three methods. The platform arm of a PUT is byte-equivalent to the
//! DELETE, so both route to one function rather than two that could drift.
//!
//! # The activation ladder's rungs are values, not errors
//!
//! `PUT mode=self_managed` refuses four ways a client can provoke, and each
//! answers its own registry code. None is an error: rung one is a `None` in
//! the parsed body, and the rest arrive as [`Activation`] variants the store
//! decided from rows it had already read. So this crate keeps no error type —
//! the shape `RUST_ERROR_STANDARD` records for every plane crate — and the
//! codes are chosen where the fact is known rather than by matching on a
//! datastore failure's neighbours.
//!
//! # The view is composed from two independent facts
//!
//! What this tenant selected, and whether the deployment has an active
//! platform default to fall back on. The Zig reads them on two connections
//! because its driver cannot nest a query inside an open result set; here they
//! are two seam reads and the composition is visible where it is decided:
//!
//!   row?                → the stored row, whatever its mode
//!   no row + default    → the live default, rendered as platform mode
//!   no row + no default → the empty view — "not configured", never a 404
//!
//! # The reset answers from what it already read
//!
//! Resetting requires the active default (there is nothing to reset TO
//! without one), so the handler is already holding the row the response
//! renders. No re-read: the response echoes the write, which a racing writer
//! cannot photobomb.

use std::sync::Arc;

use afd_billing::Posture;
use afd_credential::provider::{PlatformDefault, Selection};
use afd_wire::tenant_provider::{ProviderMode, TenantProviderResponse};
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::services::{Services, TenantProviders as _};

use super::tenant_of;

/// The scoped events each verb's failures are logged under.
const EVENT_VIEW: &str = "provider_view_failed";
const EVENT_RESET: &str = "provider_reset_failed";
const EVENT_APPLY: &str = "provider_apply_failed";
const EVENT_TENANT: &str = "provider_tenant_unresolved";

/// The sentence a reset with no active platform default earns.
///
/// `pub` so the router suite asserts it by identity (RULE UFS). Wire-visible
/// and deliberately schema-free: this handler needs no operator scope, so the
/// sentence must not leak what table the default lives in.
pub const DETAIL_PLATFORM_KEY_MISSING: &str = "Platform LLM key not configured";

/// The sentence a `self_managed` body naming no credential earns.
pub const DETAIL_SECRET_REF_REQUIRED: &str = "secret_ref required when mode=self_managed";

/// The sentence a credential the vault does not hold earns.
pub const DETAIL_SECRET_NOT_FOUND: &str = "credential row not found in vault";

/// The sentence a credential that is not a provider key earns.
///
/// One sentence for two shapes — a body that will not read as a credential,
/// and a row whose metadata says it is not a provider key — because to a
/// caller they are the same repair: store a provider credential under that
/// name. The Zig answers this code for both.
pub const DETAIL_SECRET_DATA_MALFORMED: &str =
    "credential JSON missing required field (provider, or api_key for a named provider)";

/// The sentence a model the catalogue does not carry earns.
pub const DETAIL_MODEL_NOT_IN_CATALOGUE: &str = "model not in cached caps catalogue";

/// The sentence a tenant with no workspace earns.
pub const DETAIL_NO_PRIMARY_WORKSPACE: &str = "Tenant has no primary workspace configured";

/// The refusal a body this daemon cannot read earns.
pub const DETAIL_MALFORMED_BODY: &str = "Malformed JSON";

/// What the empty view renders when nothing is configured anywhere.
///
/// The Zig serves empty strings rather than a 404 or a hardcoded model, so the
/// dashboard shows "not configured" instead of a stale default. Kept to the
/// byte.
const NOT_CONFIGURED: &str = "";

/// `GET /v1/tenants/me/provider` — the persisted selection, never a key.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/provider",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "get_tenant_provider",
    summary = "Read tenant model provider settings",
    description = concat!(
        "Returns the tenant's provider mode, provider name, model, context ",
        "limit, and credential reference. Secret values are never returned. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = TenantProviderResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn view<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let tenant = tenant_of(
        &services,
        person,
        super::DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let store = services.tenant_providers();
    let selection = store
        .selection(&tenant)
        .await
        .map_err(Refusal::at(EVENT_VIEW))?;
    let default = store
        .platform_default()
        .await
        .map_err(Refusal::at(EVENT_VIEW))?;

    let available = default.is_some();
    Ok(match (&selection, &default) {
        (Some(own), _) => Json(from_selection(own, available)).into_response(),
        (None, Some(fallback)) => Json(from_default(fallback, available)).into_response(),
        (None, None) => Json(empty_view()).into_response(),
    })
}

/// The stored row, rendered.
pub(super) fn from_selection(
    own: &Selection,
    platform_default_available: bool,
) -> TenantProviderResponse<'_> {
    TenantProviderResponse {
        mode: match own.posture {
            Posture::Platform => ProviderMode::Platform,
            Posture::SelfManaged => ProviderMode::SelfManaged,
        },
        provider: &own.provider,
        model: &own.model,
        context_cap_tokens: own.context_cap_tokens,
        secret_ref: own.secret_ref.as_deref(),
        platform_default_available,
    }
}

/// The live platform default, rendered for a tenant with no row of its own.
fn from_default(
    fallback: &PlatformDefault,
    platform_default_available: bool,
) -> TenantProviderResponse<'_> {
    TenantProviderResponse {
        mode: ProviderMode::Platform,
        provider: &fallback.provider,
        model: &fallback.model,
        context_cap_tokens: fallback.context_cap_tokens,
        secret_ref: None,
        platform_default_available,
    }
}

/// Nothing configured anywhere: platform mode over empty names.
const fn empty_view() -> TenantProviderResponse<'static> {
    TenantProviderResponse {
        mode: ProviderMode::Platform,
        provider: NOT_CONFIGURED,
        model: NOT_CONFIGURED,
        context_cap_tokens: 0,
        secret_ref: None,
        platform_default_available: false,
    }
}

pub(crate) mod write;

#[cfg(test)]
mod tests;
