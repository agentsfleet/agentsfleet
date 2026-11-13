//! Delete a fleet in any status; native reads and edits live in `native`.

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::{Services, WorkspaceFleets as _};
use afd_http::handler::authoring_refusal;
pub use afd_http::handler::{DETAIL_FLEET_ID, FleetPath, parse_fleet_id};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

const EVENT_PURGE: &str = "fleet_purge_failed";

/// `DELETE /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — purge a fleet in any status.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "delete_fleet",
    summary = "Permanently delete a fleet",
    description = concat!(
        "Permanently deletes the fleet, events, runs, approvals, grants, ",
        "keys, and memory, in any status. A running fleet's setup and live ",
        "work end first. This cannot be undone. Archive is a separate ",
        "operation: set the status to `archived` to keep the history instead. ",
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
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let expected = crate::handler::input::expected(&headers)?.ok_or_else(|| {
        Refusal::coded(
            af_core::error_code::AUTHORING_PRECONDITION,
            "Send the current fleet If-Match tag",
        )
    })?;
    // The teardown an archive runs, so a fleet deleted while it works stops
    // working; it writes no status, so the purge's tag check is unchanged.
    services
        .creations()
        .release(
            afd_authoring::Access {
                workspace: &owned.workspace,
                subject: person.subject(),
            },
            &fleet,
            expected,
        )
        .await
        .map_err(authoring_refusal)?;
    services
        .fleets()
        .purge_tagged(&owned.workspace, &fleet, expected)
        .await
        .map_err(|error| match error.stale_tag() {
            Some(etag) => Refusal::preconditioned(error.code(), error.detail(), etag),
            None => Refusal::at(EVENT_PURGE)(error),
        })?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
