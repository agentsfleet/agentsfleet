//! Save returns the retained fleet even while prerequisites remain pending.

use super::{access, input, render};
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    services::Services,
};
use afd_http::handler::{Refusal, authoring_refusal};
use axum::{body::Bytes, extract::State, response::Response};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = "/v1/workspaces/{workspace_id}/fleets", operation_id = "create_fleet",
    tag = afd_http::openapi::tag::FLEETS, summary = "Save a reviewed fleet",
    description = "Saves an idle private candidate or pinned Library revision. Exact retries resume one retained fleet. Installing progress names live prerequisites. Run-now acceptance additionally requires a saved queue receipt.",
    params(afd_http::openapi::path::Workspace, ("Idempotency-Key" = String, Header, description = "Required UUIDv7 creation key.")),
    request_body = afd_api_wire::fleet::SaveFleetRequest,
    responses((status = 201, body = afd_api_wire::fleet::FleetResponse, description = "Retained fleet", headers(("Location" = String), ("ETag" = String))),
        (status = 400, description = "Invalid source or choices"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Source unavailable"),
        (status = 409, description = "Review or operation changed"), (status = 410, description = "Accepted fleet purged"),
        (status = 413, description = "Body too large"), (status = 429, description = "Too many requests"),
        (status = 500, description = "Internal failure"), (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn save<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let operation = input::operation(&headers)?;
    let request = input::save(&body)?;
    let saved = services
        .creations()
        .save(access(&owned, &person), &operation, request, services.now())
        .await
        .map_err(authoring_refusal)?;
    super::schedules::after_activation(
        services.as_ref(),
        &owned.workspace,
        &saved.id,
        saved.status == "active",
    )
    .await?;
    render::response(
        services.as_ref(),
        &owned.workspace,
        saved,
        StatusCode::CREATED,
    )
    .await
}
