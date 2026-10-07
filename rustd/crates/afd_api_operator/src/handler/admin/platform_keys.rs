//! Reveal-free platform-default HTTP adapters.

use std::borrow::Cow;
use std::sync::Arc;

use afd_admin::{PlatformKey, PlatformKeyInput, SetPlatformKey};
use afd_api_wire::admin::{
    KEY_PROVIDER_MAX_BYTES, PlatformKeyDeactivateResponse, PlatformKeyItem, PlatformKeyPut,
    PlatformKeySetResponse, PlatformKeysResponse,
};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_validate::Sentences;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use crate::auth::PersonIdentity;
use crate::handler::{refuse, reject};
use crate::request_id::RequestId;
use crate::services::Services;

const DETAIL_BODY_REQUIRED: &str = "Request body required";
const DETAIL_MALFORMED_JSON: &str = "Malformed JSON";
const DETAIL_PROVIDER_LEN: &str = "provider must be 1–32 chars";
const DETAIL_MODEL_LEN: &str = "model must be 1–256 chars";
const DETAIL_WORKSPACE_ID: &str = "source_workspace_id must be a canonical UUIDv7";
const DETAIL_BASE_URL: &str = "base_url invalid: openai-compatible needs an https SSRF-safe URL; a named provider must omit it";
const DETAIL_WORKSPACE_UNKNOWN: &str =
    "source_workspace_id does not reference an existing workspace";
const DETAIL_MODEL_UNKNOWN: &str =
    "model is not a priced catalogue row for this provider; add it to /admin/models first";

/// Lists all active and inactive metadata without reading vault bytes.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/admin/platform-keys",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "list_platform_keys",
    summary = "List platform LLM keys",
    description = concat!(
        "Lists all configured platform-wide LLM provider keys. These are ",
        "shared keys that workspaces fall back to when no self-managed key is ",
        "configured. ",
    ),
    params(
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PlatformKeysResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(State(services): State<Arc<D>>) -> Response {
    match services.platform_keys().list().await {
        Ok(keys) => Json(PlatformKeysResponse {
            keys: keys.iter().map(item).collect(),
            request_id: request_id(),
        })
        .into_response(),
        Err(error) => refuse(&error, "admin_platform_keys_list_failed"),
    }
}

/// Activates one provider/model pair as the sole platform default.
#[cfg_attr(feature = "openapi", utoipa::path(
    put,
    path = "/v1/admin/platform-keys",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "put_platform_key",
    summary = "Set a platform LLM key",
    description = concat!(
        "Configures a platform-wide LLM provider key by referencing a ",
        "workspace that has a self-managed key stored. The platform key ",
        "proxies through the referenced workspace's credential. ",
    ),
    request_body = PlatformKeyPut,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PlatformKeySetResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn set<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Response {
    let request = match request(&body) {
        Ok(request) => request,
        Err((code, detail)) => return reject(code, detail),
    };
    let provider = request.provider.into_owned();
    let model = request.model.into_owned();
    let workspace = request.source_workspace_id;
    let input = PlatformKeyInput::new(
        provider.clone(),
        workspace.clone(),
        model.clone(),
        request.base_url.map(Cow::into_owned),
    );
    match services.platform_keys().set(&input, services.now()).await {
        Ok(SetPlatformKey::Set(key)) => {
            tracing::info!(
                actor_id = identity.subject(),
                provider = key.provider(),
                model = key.model(),
                event = "admin_platform_default_set",
            );
            Json(PlatformKeySetResponse {
                provider: Cow::Owned(provider),
                model: Cow::Owned(model),
                source_workspace_id: Cow::Owned(workspace.to_string()),
                active: true,
                request_id: request_id(),
            })
            .into_response()
        }
        Ok(SetPlatformKey::WorkspaceNotFound) => reject(
            error_code::PROVIDER_SOURCE_WORKSPACE_NOT_FOUND,
            DETAIL_WORKSPACE_UNKNOWN,
        ),
        Ok(SetPlatformKey::ModelNotFound) => reject(
            error_code::PROVIDER_MODEL_NOT_IN_CATALOGUE,
            DETAIL_MODEL_UNKNOWN,
        ),
        Err(error) => refuse(&error, "admin_platform_key_set_failed"),
    }
}

/// Deactivates a provider whether or not it currently has a row.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/admin/platform-keys/{provider}",
    tag = afd_http::openapi::tag::ADMIN,
    operation_id = "delete_platform_key",
    summary = "Delete a platform LLM key",
    description = concat!(
        "Removes a platform-wide LLM provider key. Workspaces without ",
        "self-managed keys will no longer be able to use this provider. ",
    ),
    params(
        afd_http::openapi::path::Provider,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PlatformKeyDeactivateResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn deactivate<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(provider): Path<String>,
) -> Response {
    let segment = ProviderSegment { provider };
    if let Err(report) = segment.validate() {
        return reject(error_code::INVALID_REQUEST, BOUNDS.pick(&report));
    }
    let provider = segment.provider;
    match services
        .platform_keys()
        .deactivate(&provider, services.now())
        .await
    {
        Ok(_existed) => {
            tracing::info!(
                actor_id = identity.subject(),
                provider,
                event = "admin_platform_key_deactivated",
            );
            Json(PlatformKeyDeactivateResponse {
                provider: Cow::Owned(provider),
                active: false,
                request_id: request_id(),
            })
            .into_response()
        }
        Err(error) => refuse(&error, "admin_platform_key_deactivate_failed"),
    }
}

/// The sentence a caller is told, keyed by the path of the bound their body
/// broke.
///
/// The wording is a public commitment the dashboard renders, so the bound
/// lives on the wire type and only the sentence stays here. A report naming
/// neither field cannot blame one, so it answers as malformed.
const BOUNDS: Sentences = Sentences::new(
    &[
        (FIELD_PROVIDER, DETAIL_PROVIDER_LEN),
        (FIELD_MODEL, DETAIL_MODEL_LEN),
    ],
    DETAIL_MALFORMED_JSON,
);

/// The path `garde` reports a provider-length break under, on the body and on
/// the `DELETE` path alike.
const FIELD_PROVIDER: &str = "provider";

/// The provider a `DELETE` path names, bounded like the body's field.
///
/// The bound is the wire type's own constant: the provider is a vault row
/// name either way, so the path cannot address a row the body could not set.
#[derive(Debug, garde::Validate)]
struct ProviderSegment {
    #[garde(length(bytes, min = 1, max = KEY_PROVIDER_MAX_BYTES))]
    provider: String,
}
/// The path `garde` reports a model-length break under.
const FIELD_MODEL: &str = "model";

#[derive(Debug, PartialEq, Eq)]
struct Validated<'a> {
    provider: Cow<'a, str>,
    source_workspace_id: Uuid7,
    model: Cow<'a, str>,
    base_url: Option<Cow<'a, str>>,
}

fn request(body: &[u8]) -> Result<Validated<'_>, (error_code::ErrorCode, &'static str)> {
    if body.is_empty() {
        return Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED));
    }
    let request = afd_http::handler::read_body::<PlatformKeyPut<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    request
        .validate()
        .map_err(|report| (error_code::INVALID_REQUEST, BOUNDS.pick(&report)))?;
    let source_workspace_id = Uuid7::parse(&request.source_workspace_id)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_WORKSPACE_ID))?;
    afd_credential::provider::validate_endpoint_pair(
        &request.provider,
        request.base_url.as_deref(),
    )
    .map_err(|_rejection| (error_code::PROVIDER_BASE_URL_INVALID, DETAIL_BASE_URL))?;
    Ok(Validated {
        provider: request.provider,
        source_workspace_id,
        model: request.model,
        base_url: request.base_url,
    })
}

fn item(key: &PlatformKey) -> PlatformKeyItem<'static> {
    PlatformKeyItem {
        provider: Cow::Owned(key.provider().to_owned()),
        source_workspace_id: Cow::Owned(key.source_workspace_id().to_string()),
        model: key.model().map(|model| Cow::Owned(model.to_owned())),
        active: key.is_active(),
        updated_at: key.updated_at().as_millis(),
    }
}

fn request_id() -> Cow<'static, str> {
    Cow::Owned(RequestId::mint().to_string())
}

#[cfg(test)]
mod tests;
