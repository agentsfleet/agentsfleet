//! The three verbs that change the registry.
//!
//! Split out of [`super`], which is at the length cap. Each one resolves the
//! caller's tenant, decides its input bounds before a pool is touched — the
//! module note on [`super`] argues why that ordering is what makes the bounds
//! provable without Postgres — and maps one store outcome onto one status.

use std::sync::Arc;

use afd_core::error_code;
use afd_credential::provider::{Added, Removed, Retargeted};
/// Named only by the `body =` clause of this module's `utoipa::path`
/// annotations, which the default build compiles away — so the import has to
/// go with them or the feature-off build fails on an unused name.
#[cfg(feature = "openapi")]
use afd_wire::tenant_model_entry::StoredModelEntry;
use afd_wire::tenant_model_entry::{CreateModelEntryRequest, UpdateModelEntryRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::PersonIdentity;
use crate::handler::Refusal;
// Two sentences the provider surface already owns, byte-identical here because
// the fact is the same one: a body this daemon cannot read, and a tenant whose
// bootstrap never produced a workspace (RULE UFS).
use crate::handler::tenant::provider::{DETAIL_MALFORMED_BODY, DETAIL_NO_PRIMARY_WORKSPACE};
use crate::handler::tenant::{DETAIL_TENANT_REQUIRED, tenant_of};
use crate::services::{Services, TenantModelEntries as _};

use super::input::{bounded_model, parse_entry_id};
use super::render::stored;
use super::{
    DETAIL_DELETE_ACTIVE, DETAIL_DUPLICATE_ENTRY, DETAIL_ENTRY_NOT_FOUND,
    DETAIL_SECRET_REF_REQUIRED, DETAIL_SECRET_REF_UNKNOWN, EVENT_CREATE, EVENT_DELETE,
    EVENT_TENANT, EVENT_UPDATE,
};

/// `POST /v1/tenants/me/models` — register a model on a stored credential.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/tenants/me/models",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "create_tenant_model_entry",
    summary = "Register a model entry",
    description = concat!(
        "Creates one `(model_id, secret_ref)` row. `secret_ref` must already ",
        "name a vault secret in the tenant's primary workspace — store or ",
        "reuse one via `/v1/workspaces/{workspace_id}/secrets` first. Does ",
        "not activate the entry; activate via `PUT /v1/tenants/me/provider`. ",
    ),
    request_body = CreateModelEntryRequest,
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = StoredModelEntry),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: CreateModelEntryRequest = afd_core::json::object_from_slice(&body)
        .map_err(|_shape| Refusal::malformed(DETAIL_MALFORMED_BODY))?;
    let model = bounded_model(&request.model_id)?;
    if request.secret_ref.is_empty() {
        return Err(Refusal::malformed(DETAIL_SECRET_REF_REQUIRED));
    }

    let tenant = tenant_of(
        &services,
        identity.person(),
        DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let outcome = services
        .tenant_providers()
        .add_entry(&tenant, model, &request.secret_ref, services.now())
        .await
        .map_err(Refusal::at(EVENT_CREATE))?;

    match outcome {
        Added::Stored(entry) => Ok((StatusCode::CREATED, Json(stored(&entry))).into_response()),
        Added::CredentialMissing => Err(Refusal::coded(
            error_code::MODELS_SECRET_NOT_FOUND,
            DETAIL_SECRET_REF_UNKNOWN,
        )),
        Added::Duplicate => Err(Refusal::coded(
            error_code::MODELS_DUPLICATE_ENTRY,
            DETAIL_DUPLICATE_ENTRY,
        )),
        Added::NoWorkspace => Err(Refusal::coded(
            error_code::TENANT_NO_PRIMARY_WORKSPACE,
            DETAIL_NO_PRIMARY_WORKSPACE,
        )),
    }
}

/// `PATCH /v1/tenants/me/models/{id}` — point an entry at another model.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/tenants/me/models/{id}",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "update_tenant_model_entry",
    summary = "Change a model entry's model_id",
    description = concat!(
        "Model-only change; `secret_ref` is immutable on this endpoint — ",
        "create a new entry to point at a different secret. ",
    ),
    request_body = UpdateModelEntryRequest,
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = StoredModelEntry),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(entry_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let entry = parse_entry_id(&entry_id)?;
    let request: UpdateModelEntryRequest = afd_core::json::object_from_slice(&body)
        .map_err(|_shape| Refusal::malformed(DETAIL_MALFORMED_BODY))?;
    let model = bounded_model(&request.model_id)?;

    let tenant = tenant_of(
        &services,
        identity.person(),
        DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    let outcome = services
        .tenant_providers()
        .set_entry_model(&tenant, &entry, model, services.now())
        .await
        .map_err(Refusal::at(EVENT_UPDATE))?;

    match outcome {
        Retargeted::Stored(entry) => Ok(Json(stored(&entry)).into_response()),
        Retargeted::NotFound => Err(Refusal::coded(
            error_code::MODELS_ENTRY_NOT_FOUND,
            DETAIL_ENTRY_NOT_FOUND,
        )),
        Retargeted::Duplicate => Err(Refusal::coded(
            error_code::MODELS_DUPLICATE_ENTRY,
            DETAIL_DUPLICATE_ENTRY,
        )),
    }
}

/// `DELETE /v1/tenants/me/models/{id}` — remove an entry.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/models/{id}",
    tag = afd_http::openapi::tag::TENANT,
    operation_id = "delete_tenant_model_entry",
    summary = "Remove a model entry",
    description = concat!(
        "Idempotent — deleting an id that doesn't exist (already removed, or ",
        "never existed) still returns 204. The referenced vault secret is ",
        "never touched; sibling entries sharing it survive. ",
    ),
    params(
        afd_http::openapi::path::Id,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(entry_id): Path<String>,
) -> Result<Response, Refusal> {
    let entry = parse_entry_id(&entry_id)?;
    let tenant = tenant_of(
        &services,
        identity.person(),
        DETAIL_TENANT_REQUIRED,
        EVENT_TENANT,
    )
    .await?;

    match services
        .tenant_providers()
        .remove_entry(&tenant, &entry)
        .await
        .map_err(Refusal::at(EVENT_DELETE))?
    {
        Removed::Done => Ok(StatusCode::NO_CONTENT.into_response()),
        Removed::Active => Err(Refusal::coded(
            error_code::MODELS_DELETE_ACTIVE,
            DETAIL_DELETE_ACTIVE,
        )),
    }
}
