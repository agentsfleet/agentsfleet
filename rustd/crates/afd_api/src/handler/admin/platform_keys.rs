//! Reveal-free platform-default HTTP adapters.

use std::borrow::Cow;
use std::sync::Arc;

use afd_admin::{PlatformKey, PlatformKeyInput, SetPlatformKey};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::admin::{
    PlatformKeyDeactivateResponse, PlatformKeyItem, PlatformKeyPut, PlatformKeySetResponse,
    PlatformKeysResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

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
            tracing::debug!(
                actor_id = identity.0.subject().as_str(),
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
        Ok(SetPlatformKey::WorkspaceNotFound) => {
            reject(error_code::INVALID_REQUEST, DETAIL_WORKSPACE_UNKNOWN)
        }
        Ok(SetPlatformKey::ModelNotFound) => reject(
            error_code::PROVIDER_MODEL_NOT_IN_CATALOGUE,
            DETAIL_MODEL_UNKNOWN,
        ),
        Err(error) => refuse(&error, "admin_platform_key_set_failed"),
    }
}

/// Deactivates a provider whether or not it currently has a row.
pub(crate) async fn deactivate<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(provider): Path<String>,
) -> Response {
    if provider.is_empty() || provider.len() > 32 {
        return reject(error_code::INVALID_REQUEST, DETAIL_PROVIDER_LEN);
    }
    match services
        .platform_keys()
        .deactivate(&provider, services.now())
        .await
    {
        Ok(_existed) => {
            tracing::debug!(
                actor_id = identity.0.subject().as_str(),
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
    let request = afd_core::json::object_from_slice::<PlatformKeyPut<'_>>(body)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))?;
    if request.provider.is_empty() || request.provider.len() > 32 {
        return Err((error_code::INVALID_REQUEST, DETAIL_PROVIDER_LEN));
    }
    if request.model.is_empty() || request.model.len() > 256 {
        return Err((error_code::INVALID_REQUEST, DETAIL_MODEL_LEN));
    }
    let source_workspace_id = Uuid7::parse(&request.source_workspace_id)
        .map_err(|_error| (error_code::INVALID_REQUEST, DETAIL_WORKSPACE_ID))?;
    afd_fleet::provider::validate_endpoint_pair(&request.provider, request.base_url.as_deref())
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
mod tests {
    use super::*;

    const WORKSPACE: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e9d02";

    #[test]
    fn request_validation_pins_pairing_and_bounds() {
        let named = format!(
            r#"{{"provider":"anthropic","source_workspace_id":"{WORKSPACE}","model":"claude-opus-5","base_url":null}}"#
        );
        assert_eq!(request(named.as_bytes()).map(|_request| ()), Ok(()));

        let compatible = format!(
            r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE}","model":"custom","base_url":"https://models.example/v1"}}"#
        );
        assert_eq!(request(compatible.as_bytes()).map(|_request| ()), Ok(()));
        assert_eq!(
            request(b""),
            Err((error_code::INVALID_REQUEST, DETAIL_BODY_REQUIRED))
        );
        assert_eq!(
            request(b"[]"),
            Err((error_code::INVALID_REQUEST, DETAIL_MALFORMED_JSON))
        );

        let unsafe_url = format!(
            r#"{{"provider":"openai-compatible","source_workspace_id":"{WORKSPACE}","model":"custom","base_url":"https://127.0.0.1/v1"}}"#
        );
        assert_eq!(
            request(unsafe_url.as_bytes()),
            Err((error_code::PROVIDER_BASE_URL_INVALID, DETAIL_BASE_URL))
        );
    }
}
