//! `DELETE /v1/fleets/runners/{runner_id}` — retire a revoked runner's record.
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
use crate::handler::{parse_id, refuse};
use crate::services::Services;

/// The scoped event a failed retirement is logged under.
const EVENT: &str = "runner_delete_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/fleets/runners/{runner_id}",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "delete_fleet_runner",
    summary = "Delete a revoked runner",
    description = concat!(
        "Permanently deletes a revoked runner, its leases, and runner events. Revoke it ",
        "first with a PATCH request setting `action` to `revoke`. Active leases must finish ",
        "or expire before deletion. Fleets previously assigned to this runner can then use ",
        "another runner.",
    ),
    params(
        afd_http::openapi::path::Runner,
    ),
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
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    Path(raw): Path<String>,
) -> Response {
    let runner = match parse_id(&raw, query::DETAIL_BAD_RUNNER_ID) {
        Ok(runner) => runner,
        Err(refused) => return refused.into_response(),
    };
    match services.runners().delete_revoked(&runner).await {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
