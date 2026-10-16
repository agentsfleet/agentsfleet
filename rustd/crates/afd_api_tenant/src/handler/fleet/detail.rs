//! Explicit purge after Kill; native reads and edits live in `native`.

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceFleets as _};
pub use afd_http::handler::{DETAIL_FLEET_ID, FleetPath, parse_fleet_id};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

const EVENT_PURGE: &str = "fleet_purge_failed";

/// `DELETE /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — purge a killed fleet.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "delete_fleet",
    summary = "Permanently delete a fleet",
    description = concat!(
        "Permanently deletes the fleet, events, runs, approvals, grants, ",
        "keys, and memory. This cannot be undone. Set the fleet status to ",
        "`killed` first. Otherwise the request returns 409. Requires an ",
        "operator role. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
        ("If-Match" = String, Header, description = "Required current resource tag"),
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = "Stale resource tag"),
        (status = 428, description = "Current resource tag required"),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn purge<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let expected = crate::handler::input::expected(&headers)?.ok_or_else(|| {
        Refusal::coded(
            afd_core::error_code::AUTHORING_PRECONDITION,
            "Send the current fleet If-Match tag",
        )
    })?;
    services
        .fleets()
        .purge_tagged(&owned.workspace, &fleet, expected)
        .await
        .map_err(|error| {
            if let Some(etag) = error.stale_tag() {
                Refusal::preconditioned(error.code(), error.detail(), etag)
            } else if error.code() == afd_core::error_code::AGENTSFLEET_ALREADY_TERMINAL {
                Refusal::conflict_at(EVENT_PURGE, "kill_required")(error)
            } else {
                Refusal::at(EVENT_PURGE)(error)
            }
        })?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
