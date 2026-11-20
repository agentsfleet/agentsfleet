//! The calling tenant's own runners, under `/v1/tenants/me/runners`.
//!
//! Each handler resolves the caller's tenant and calls the same core its
//! platform twin calls, with the tenant's scope, so the two surfaces cannot drift
//! apart. A runner outside the tenant answers `404`, as one that does not exist.

use std::collections::HashMap;
use std::sync::Arc;

use af_core::id::Uuid7;
// Named only in the OpenAPI annotations below.
#[cfg(feature = "openapi")]
use af_wire::runner::{RegisterRequest, RegisterResponse};
#[cfg(feature = "openapi")]
use afd_api_wire::admin::{RunnerAdminPatchRequest, RunnerPatchReply};
#[cfg(feature = "openapi")]
use afd_api_wire::operator::{RunnerDetail, RunnersResponse};
use afd_http::handler::Refusal;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::response::{IntoResponse, Response};

use crate::auth::PersonIdentity;
use crate::services::Services;

/// The caller's tenant, or the response that refuses it.
async fn tenant<D: Services>(services: &D, identity: &PersonIdentity) -> Result<Uuid7, Refusal> {
    afd_http::handler::caller_tenant::of(services, identity.person()).await
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
        "which a platform admin grants per account. A host registers once: `host_id` is unique ",
        "within the tenant, and `Idempotency-Key` is not used. Registering a host again answers ",
        "409 with `current_state` `registered` and `Location` naming its runner. The token ",
        "appears only once, so recover it with that runner's `rotate` action. Durable tasks are ",
        "stored off whatever the body asks; only a platform admin turns them on.",
    ),
    request_body = RegisterRequest<'_>,
    responses(
        (status = 201, description = "Runner registered; the bearer token is returned exactly once", body = RegisterResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 409, description = "This host is already registered; `Location` names its runner",
            headers(("Location" = String, description = "The runner the host is registered as"))),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn register<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    body: Bytes,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => {
            super::runner_register::tenant(&*services, &tenant, identity.subject(), &body).await
        }
        Err(refused) => refused.into_response(),
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
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runners::page(services.runners().tenant(&tenant), &params).await,
        Err(refused) => refused.into_response(),
    }
}

/// Reads one of the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::TENANT_RUNNER,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "get_tenant_runner",
    summary = "Read a tenant runner",
    description = "Returns one of the calling tenant's runners. Another tenant's runner answers 404. Requires `runner:tenant`.",
    params(afd_http::openapi::path::Runner),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RunnerDetail<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn detail<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => super::runners::read(services.runners().tenant(&tenant), &raw).await,
        Err(refused) => refused.into_response(),
    }
}

/// Applies an action or an edit to one of the caller's runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = af_wire::paths::TENANT_RUNNER,
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
        (status = 200, description = "The runner's state after the change, or its replacement token after a `rotate`", body = RunnerPatchReply<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is revoked, so the request can never be honoured"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
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
            super::runner_patch::apply(
                &*services,
                services.runners().tenant(&tenant),
                &identity,
                &raw,
                &body,
            )
            .await
        }
        Err(refused) => refused.into_response(),
    }
}

/// Retires one of the caller's revoked runners.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = af_wire::paths::TENANT_RUNNER,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "delete_tenant_runner",
    summary = "Delete a revoked tenant runner",
    description = "Permanently deletes one of the calling tenant's revoked runners. Another tenant's runner answers 404. Requires `runner:tenant`.",
    params(afd_http::openapi::path::Runner),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is still in service; revoke it first"),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn delete<D: Services>(
    State(services): State<Arc<D>>,
    identity: PersonIdentity,
    Path(raw): Path<String>,
) -> Response {
    match tenant(&*services, &identity).await {
        Ok(tenant) => {
            super::runner_delete::retire(&*services, services.runners().tenant(&tenant), &raw).await
        }
        Err(refused) => refused.into_response(),
    }
}
