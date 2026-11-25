//! A person's change to a fleet: its name, archive, model and network
//! toggles, under the row's strong tag. Only a signed-in person opens the
//! network.

use super::{access, input, render};
use crate::{
    auth::{PersonIdentity, SignedInClass, WorkspaceContext},
    handler::{
        fleet::detail::{FleetPath, parse_fleet_id},
        input::expected,
    },
    services::TenantPlane,
};
use afd_http::handler::{Refusal, setup_refusal};
use axum::{
    body::Bytes,
    extract::{Path, State},
    response::Response,
};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}", operation_id = "patch_fleet",
    tag = afd_http::openapi::tag::FLEETS, summary = "Change a fleet's name, model, network or lifecycle",
    description = concat!(
        "Updates the name, the model the next run uses, the network toggles or the lifecycle. ",
        "Send the current `ETag` in `If-Match`. A model change writes one `model_changed` ",
        "turn. `http_egress` and `package_domains` need a signed-in person; an API key ",
        "receives 403 and nothing changes. Archiving ends the fleet's live work and is terminal.",
    ),
    params(afd_http::openapi::path::Fleet, ("If-Match" = String, Header, description = "Required current strong resource tag.")),
    request_body = afd_api_wire::fleet::PatchFleetRequest,
    responses((status = 200, body = afd_api_wire::fleet::FleetResponse<'_>, description = "Current fleet", headers(("ETag" = String))),
        (status = 400, description = "Invalid mutation"),
        (status = 403, description = "An API key changed a network toggle (`UZ-AUTH-001`)"),
        (status = 404, description = "No such fleet in this workspace"),
        (status = 409, description = "The fleet is archived (`UZ-AGT-020`, `current_state: archived`), the model is not one the workspace can use (`UZ-SETUP-008`), or the name is taken (`UZ-AGT-006`)"),
        (status = 412, description = "Stale tag"),
        (status = 428, description = "Missing tag"), (status = 413, description = "Body too large"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn change<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let request = input::patch(&body)?;
    if request.opens_network() {
        person.narrow::<SignedInClass>()?;
    }
    let expected = expected(&headers)?.ok_or_else(|| {
        Refusal::coded(
            af_core::error_code::SETUP_PRECONDITION,
            "Send the current fleet If-Match tag",
        )
    })?;
    let saved = services
        .creations()
        .patch(access(&owned, &person), &fleet, Some(expected), &request)
        .await
        .map_err(setup_refusal)?;
    render::response(services.as_ref(), &owned.workspace, saved, StatusCode::OK).await
}
