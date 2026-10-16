//! Shared fleet reads expose current setup without the private conversation.

use super::{access, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::fleet::detail::{FleetPath, parse_fleet_id},
    services::Services,
};
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{
    extract::{Path, State},
    response::Response,
};
use http::StatusCode;
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}", operation_id = "get_fleet",
    tag = afd_http::openapi::tag::FLEETS, summary = "Read fleet setup and progress",
    params(afd_http::openapi::path::Fleet),
    responses((status = 200, body = afd_api_wire::fleet::FleetResponse, description = "Current fleet", headers(("ETag" = String))),
        (status = 400, description = "Invalid identity"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Fleet unavailable"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let saved = services
        .creations()
        .read(access(&owned, &person), &fleet)
        .await
        .map_err(authoring_refusal)?;
    render::response(services.as_ref(), &owned.workspace, saved, StatusCode::OK).await
}
