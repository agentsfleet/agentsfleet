//! The calling tenant's own runners, under `/v1/tenants/me/runners`.
//!
//! Each handler resolves the caller's tenant and calls the same core its
//! platform twin calls, with a tenant reach, so the two surfaces cannot drift
//! apart. A runner outside the tenant answers `404`, as one that does not exist.

use std::collections::HashMap;
use std::sync::Arc;

use af_core::id::Uuid7;
// Named only in the OpenAPI annotations below.
#[cfg(feature = "openapi")]
use af_wire::runner::{RegisterRequest, RegisterResponse, RegistrationReplay};
#[cfg(feature = "openapi")]
use afd_api_wire::admin::{RunnerAdminPatchRequest, RunnerAdminPatchResponse};
#[cfg(feature = "openapi")]
use afd_api_wire::operator::{RunnerDetail, RunnersResponse};
use afd_runner::Reach;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};
use http::HeaderMap;

use crate::auth::PersonIdentity;
use crate::services::Services;

/// The caller's tenant, or the response that refuses it.
async fn tenant<D: Services>(services: &D, identity: &PersonIdentity) -> Result<Uuid7, Response> {
    afd_http::handler::reach::tenant(services, identity.person())
        .await
        .map_err(IntoResponse::into_response)
}

/// Registers one of the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::TENANT_RUNNERS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "register_tenant_runner",
    summary = "Register a tenant runner",
    description = concat!(
        "Registers a runner that takes only the calling tenant's work. Requires `runner:tenant`, ",
        "which a platform admin grants per account. Send a UUIDv7 `Idempotency-Key`: a retry ",
        "with the same key and body answers 200 with the same runner and no token, since the ",
        "token appears once; rotate it to recover. The same key with another body answers 409.",
    ),
    params(("Idempotency-Key" = String, Header, description = "Required UUIDv7 registration key.")),
    request_body = RegisterRequest<'_>,
    responses(
        (status = 201, description = "Runner registered; the bearer token is returned exactly once", body = RegisterResponse<'_>),
        (status = 200, description = "A retry of a registration that already succeeded; no token", body = RegistrationReplay<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = "This Idempotency-Key already registered a runner with a different request"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn register<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runner_register::tenant(&*services, &tenant, &headers, &body).await,
        Err(refused) => refused,
    }
}

/// Lists the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::TENANT_RUNNERS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "list_tenant_runners",
    summary = "List tenant runners",
    description = "Lists the calling tenant's runners newest first; `total` counts only them. Requires `runner:tenant`.",
    params(afd_http::openapi::query::OperatorPage),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnersResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runners::page(&*services, &Reach::Tenant(tenant), &params).await,
        Err(refused) => refused,
    }
}

/// Reads one of the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/tenants/me/runners/{runner_id}",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "get_tenant_runner",
    summary = "Read a tenant runner",
    description = "Returns one of the calling tenant's runners. Another tenant's runner answers 404. Requires `runner:tenant`.",
    params(afd_http::openapi::path::Runner),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerDetail<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn detail<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runners::read(&*services, &Reach::Tenant(tenant), &raw).await,
        Err(refused) => refused,
    }
}

/// Applies an action or an edit to one of the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/tenants/me/runners/{runner_id}",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "update_tenant_runner",
    summary = "Manage a tenant runner",
    description = concat!(
        "Takes the same body as the platform runner PATCH: an `action` (`cordon`, `drain`, `revoke`, ",
        "`rotate`, `self_test`) or an edit of `assigned_policy`, `labels`, or both. A tenant runner ",
        "never takes a decision model, and only a platform admin switches on durable tasks; both ",
        "answer 400. Another tenant's runner answers 404. Requires `runner:tenant`.",
    ),
    request_body = RunnerAdminPatchRequest<'_>,
    params(afd_http::openapi::path::Runner),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerAdminPatchResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is revoked, so the request can never be honoured"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
    body: Bytes,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => {
            super::runner_patch::apply(&*services, &Reach::Tenant(tenant), &identity, &raw, &body)
                .await
        }
        Err(refused) => refused,
    }
}

/// Retires one of the caller's revoked runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/tenants/me/runners/{runner_id}",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "delete_tenant_runner",
    summary = "Delete a revoked tenant runner",
    description = "Permanently deletes one of the calling tenant's revoked runners. Another tenant's runner answers 404. Requires `runner:tenant`.",
    params(afd_http::openapi::path::Runner),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is still in service; revoke it first"),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runner_delete::retire(&*services, &Reach::Tenant(tenant), &raw).await,
        Err(refused) => refused,
    }
}
