//! The two verbs that change which credential a tenant runs on.
//!
//! Split out of [`super`], which is at the length cap. Both resolve the
//! caller's tenant, decide a [`Selection`] and write it; the read verb and the
//! three renderings all four go through stay in [`super`], where the
//! composition they implement is written down.

use std::sync::Arc;

use afd_billing::Posture;
use afd_core::error_code;
use afd_credential::provider::{Activation, Selection};
/// Named only by the `body =` clause of this module's `utoipa::path`
/// annotations, which the default build compiles away — so the import has to
/// go with them or the feature-off build fails on an unused name.
#[cfg(feature = "openapi")]
use afd_wire::tenant_provider::TenantProviderResponse;
use afd_wire::tenant_provider::{ProviderMode, TenantProviderRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
use crate::handler::tenant::{DETAIL_TENANT_REQUIRED, tenant_of};
use crate::services::{Services, TenantProviders as _};

use super::{
    DETAIL_MALFORMED_BODY, DETAIL_MODEL_NOT_IN_CATALOGUE, DETAIL_NO_PRIMARY_WORKSPACE,
    DETAIL_PLATFORM_KEY_MISSING, DETAIL_SECRET_DATA_MALFORMED, DETAIL_SECRET_NOT_FOUND,
    DETAIL_SECRET_REF_REQUIRED, EVENT_APPLY, EVENT_RESET, EVENT_TENANT, from_selection,
};

/// `DELETE /v1/tenants/me/provider` — back to the platform default, explicitly.
///
/// Writes an explicit platform row rather than deleting the tenant's, so the
/// dashboard can tell "explicitly reset" from "never configured". The written
/// provider/model/cap are copied from the live default at reset time, which is
/// the Zig's behavior kept for parity — the divergence register carries the
/// consequence (a later repointed default is not reflected by this row's view).
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/provider",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "delete_tenant_provider",
    summary = "Reset tenant LLM provider to platform default",
    description = concat!(
        "Equivalent to `PUT {mode: \"platform\"}`. Writes the explicit ",
        "mode=platform row so the dashboard can distinguish \"never ",
        "configured\" from \"explicitly reset\". ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = TenantProviderResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn reset<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
) -> Result<Response, Refusal> {
    let person = identity.person();
    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;

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

/// `PUT /v1/tenants/me/provider` — choose the platform default or your own key.
///
/// The platform arm is the reset: same write, same response, one function.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/v1/tenants/me/provider",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "put_tenant_provider",
    summary = "Set tenant model provider",
    description = concat!(
        "Sets the provider mode and model. Self-managed mode requires a ",
        "`credential_ref` that names an existing secret. If the model entry ",
        "does not exist, this request creates it. ",
    ),
    request_body = TenantProviderRequest,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = TenantProviderResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn apply<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: TenantProviderRequest = afd_core::json::object_from_slice(&body)
        .map_err(|_shape| Refusal::malformed(DETAIL_MALFORMED_BODY))?;

    if request.mode == ProviderMode::Platform {
        return reset(State(services), identity).await;
    }

    // Rung one, and it is a `None` rather than a serde refusal: the field is
    // optional on the wire precisely so this answers a registry code with a
    // sentence, where a required field would answer a shape error naming none.
    let secret_ref = request.secret_ref.as_deref().ok_or_else(|| {
        Refusal::coded(
            error_code::PROVIDER_SECRET_REF_REQUIRED,
            DETAIL_SECRET_REF_REQUIRED,
        )
    })?;

    let person = identity.person();
    let tenant = tenant_of(&services, person, DETAIL_TENANT_REQUIRED, EVENT_TENANT).await?;

    let store = services.tenant_providers();
    let outcome = store
        .activate(
            &tenant,
            secret_ref,
            request.model.as_deref(),
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_APPLY))?;

    // One exhaustive match rather than a success check and a refusal table:
    // every outcome is answered exactly here, so a variant added to the ladder
    // fails to compile until this says what a client is told about it.
    let written = match outcome {
        Activation::Applied(written) => written,
        Activation::CredentialMissing => {
            return Err(Refusal::coded(
                error_code::PROVIDER_SECRET_NOT_FOUND,
                DETAIL_SECRET_NOT_FOUND,
            ));
        }
        // Two shapes, one answer: a body that will not read as a credential,
        // and a row whose metadata says it is not a provider key. To a caller
        // the repair is the same — store a provider credential under that name
        // — and the Zig answers this code for both.
        Activation::NotAProviderKey | Activation::Malformed => {
            return Err(Refusal::coded(
                error_code::PROVIDER_SECRET_DATA_MALFORMED,
                DETAIL_SECRET_DATA_MALFORMED,
            ));
        }
        // The guard's own classification, rendered to its stable word — never
        // the URL and never the host, which sit beside an `api_key` in the
        // same credential.
        Activation::EndpointRefused(rejection) => {
            return Err(Refusal::coded(
                error_code::PROVIDER_BASE_URL_INVALID,
                rejection.as_str(),
            ));
        }
        Activation::ModelUnknown => {
            return Err(Refusal::coded(
                error_code::PROVIDER_MODEL_NOT_IN_CATALOGUE,
                DETAIL_MODEL_NOT_IN_CATALOGUE,
            ));
        }
        Activation::NoWorkspace => {
            return Err(Refusal::coded(
                error_code::TENANT_NO_PRIMARY_WORKSPACE,
                DETAIL_NO_PRIMARY_WORKSPACE,
            ));
        }
    };

    // Whether a default EXISTS is independent of this tenant now running on
    // its own key — the Models page gates its "switch back" action on it — so
    // it is read even though nothing above needed it.
    let available = store
        .platform_default()
        .await
        .map_err(Refusal::at(EVENT_APPLY))?
        .is_some();
    Ok(Json(from_selection(&written, available)).into_response())
}
