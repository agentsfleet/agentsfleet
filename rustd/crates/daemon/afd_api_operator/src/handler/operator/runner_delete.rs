//! `DELETE /v1/admin/runners/{runner_id}` — retire a revoked runner's record.
//!
//! Mirrors the API key's revoke-then-delete lifecycle: only an already-revoked
//! row is deletable, so the destructive step stays `PATCH {"action":"revoke"}`
//! and delete merely retires the record. The scope is `runner:write`, the same
//! as revoke — retiring a dead row is strictly less consequential than taking
//! a live runner out of service.
//!
//! Not tenant-scoped, exactly as the patch verb: the trusted fleet's runners
//! carry no tenant, so a tenant predicate would match nothing, and the scope
//! rung is the whole authorization.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::query;
use crate::OperatorPlane;
use crate::handler::{parse_id, refuse};
use afd_runner::RunnerScope;

/// The scoped event a failed retirement is logged under.
const EVENT: &str = "runner_delete_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/admin/runners/{runner_id}",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "delete_platform_runner",
    summary = "Delete a revoked platform runner",
    description = concat!(
        "Permanently deletes a revoked runner, its runs, and runner events. Revoke it ",
        "first with a PATCH request setting `action` to `revoke`. Active runs must finish ",
        "or expire before deletion. Fleets previously assigned to this runner can then use ",
        "another runner.",
    ),
    params(
        afd_http::openapi::path::Runner,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "The runner is still in service; revoke it first"),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn handle<D: OperatorPlane>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
) -> Response {
    retire(&*services, services.runners().platform(), &raw).await
}

/// Retires a revoked runner within `scope`.
pub(super) async fn retire<D: OperatorPlane>(
    services: &D,
    scope: RunnerScope<'_>,
    raw: &str,
) -> Response {
    let runner = match parse_id(raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    match scope.delete_revoked(&runner).await {
        Ok(()) => {
            super::runner_notify::changed(services, &runner).await;
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}
