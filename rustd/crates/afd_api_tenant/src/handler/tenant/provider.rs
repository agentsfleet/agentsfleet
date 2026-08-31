//! `/v1/tenants/me/provider` — whose key this tenant's runs dial with.
//!
//! The read and the reset. The activation (`PUT mode=self_managed`) lands with
//! the store verb it drives; the platform arm of a PUT is byte-equivalent to
//! the DELETE and both route here.
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
const EVENT_TENANT: &str = "provider_tenant_unresolved";

/// The sentence a reset with no active platform default earns.
///
/// `pub` so the router suite asserts it by identity (RULE UFS). Wire-visible
/// and deliberately schema-free: this handler needs no operator scope, so the
/// sentence must not leak what table the default lives in.
pub const DETAIL_PLATFORM_KEY_MISSING: &str = "Platform LLM key not configured";

/// What the empty view renders when nothing is configured anywhere.
///
/// The Zig serves empty strings rather than a 404 or a hardcoded model, so the
/// dashboard shows "not configured" instead of a stale default. Kept to the
/// byte.
const NOT_CONFIGURED: &str = "";

/// `GET /v1/tenants/me/provider` — the persisted selection, never a key.
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

/// `DELETE /v1/tenants/me/provider` — back to the platform default, explicitly.
///
/// Writes an explicit platform row rather than deleting the tenant's, so the
/// dashboard can tell "explicitly reset" from "never configured". The written
/// provider/model/cap are copied from the live default at reset time, which is
/// the Zig's behavior kept for parity — the divergence register carries the
/// consequence (a later repointed default is not reflected by this row's view).
pub(crate) async fn reset<D: Services>(
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
    let default = store
        .platform_default()
        .await
        .map_err(Refusal::at(EVENT_RESET))?
        .ok_or_else(|| {
            Refusal::coded(
                afd_core::error_code::PROVIDER_PLATFORM_KEY_MISSING,
                DETAIL_PLATFORM_KEY_MISSING,
            )
        })?;

    let written = Selection {
        posture: Posture::Platform,
        provider: default.provider.clone(),
        model: default.model.clone(),
        context_cap_tokens: default.context_cap_tokens,
        secret_ref: None,
    };
    store
        .upsert(&tenant, &written, services.now())
        .await
        .map_err(Refusal::at(EVENT_RESET))?;

    // The default row exists — the refusal above is what proves it — so the
    // availability flag is true by construction, not by a second read.
    Ok(Json(from_selection(&written, true)).into_response())
}

/// The stored row, rendered.
fn from_selection(own: &Selection, platform_default_available: bool) -> TenantProviderResponse<'_> {
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
