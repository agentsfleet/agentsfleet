//! Strong resource tags fence lifecycle changes and explicit setup retries.

use super::{access, input, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::{
        fleet::detail::{FleetPath, parse_fleet_id},
        input::expected,
    },
    services::Services,
};
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}", operation_id = "patch_fleet",
    tag = afd_http::openapi::tag::FLEETS, summary = "Change fleet lifecycle or name",
    description = concat!(
        "Updates lifecycle state or name. Send the current `ETag` in `If-Match`. Setting an ",
        "installing fleet to active resumes setup checks. Killing it cancels setup and ",
        "makes its private draft editable again.",
    ),
    params(afd_http::openapi::path::Fleet, ("If-Match" = String, Header, description = "Required current strong resource tag.")),
    request_body = afd_api_wire::fleet::PatchFleetRequest,
    responses((status = 200, body = afd_api_wire::fleet::FleetResponse, description = "Current fleet", headers(("ETag" = String))),
        (status = 400, description = "Invalid mutation"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Fleet unavailable"),
        (status = 409, description = "Lifecycle refuses mutation"), (status = 412, description = "Stale tag"),
        (status = 428, description = "Missing tag"), (status = 413, description = "Body too large"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn change<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let request = input::patch(&body)?;
    let expected = expected(&headers)?.ok_or_else(|| {
        Refusal::coded(
            afd_core::error_code::AUTHORING_PRECONDITION,
            "Send the current fleet If-Match tag",
        )
    })?;
    let saved = services
        .creations()
        .patch(
            access(&owned, &person),
            &fleet,
            Some(expected),
            request.status.map(afd_api_wire::fleet::PatchStatus::as_str),
            request.name.as_deref(),
            services.now(),
        )
        .await
        .map_err(authoring_refusal)?;
    if matches!(
        request.status,
        Some(afd_api_wire::fleet::PatchStatus::Active)
    ) {
        super::schedules::after_activation(
            services.as_ref(),
            &owned.workspace,
            &fleet,
            saved.status == "active",
        )
        .await?;
    }
    render::response(services.as_ref(), &owned.workspace, saved, StatusCode::OK).await
}
