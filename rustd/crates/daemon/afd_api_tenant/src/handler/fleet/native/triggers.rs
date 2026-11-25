//! What may wake a fleet besides its schedules, as a person sees and removes
//! it. The fleet adds its own rows by tool; there is no add on this path.

use super::access;
use crate::{
    auth::{PersonIdentity, WorkspaceContext},
    handler::fleet::detail::parse_fleet_id,
    services::TenantPlane,
};
use af_core::id::Uuid7;
use afd_api_wire::fleet::TriggersResponse;
use afd_http::handler::{FleetPath, Refusal, setup_refusal};
use axum::{
    Json,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::StatusCode;
use serde::Deserialize;
use std::sync::Arc;

/// The refusal a trigger identifier that could name no row earns.
const DETAIL_TRIGGER_ID: &str = "trigger_id must be a UUIDv7";

/// The two segments beyond the workspace the item template carries.
#[derive(Debug, Deserialize)]
pub(crate) struct TriggerPath {
    /// The fleet named in the path, still text.
    fleet_id: String,
    /// The trigger named in the path, still text.
    trigger_id: String,
}

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/triggers",
    operation_id = "list_fleet_triggers",
    tag = afd_http::openapi::tag::FLEETS, summary = "List what wakes a fleet",
    description = concat!(
        "Returns the fleet's webhook, GitHub App and Slack mention triggers, oldest first. ",
        "A webhook carries the `url` its deliveries are sent to. The fleet adds triggers ",
        "itself, when a person's message asks it to.",
    ),
    params(afd_http::openapi::path::Fleet),
    responses((status = 200, body = TriggersResponse<'_>, description = "The fleet's triggers"),
        (status = 400, description = "Invalid identity"), (status = 404, description = "Fleet unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn list<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let rows = services
        .creations()
        .triggers(access(&owned, &person), &fleet)
        .await
        .map_err(setup_refusal)?;
    let deployment = services.deployment();
    Ok(Json(TriggersResponse {
        items: rows
            .iter()
            .map(|row| row.shown(deployment, &fleet))
            .collect(),
    })
    .into_response())
}

#[cfg_attr(feature = "openapi", utoipa::path(
    delete, path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/triggers/{trigger_id}",
    operation_id = "remove_fleet_trigger",
    tag = afd_http::openapi::tag::FLEETS, summary = "Remove a trigger from a fleet",
    description = concat!(
        "Removes one trigger. A delivery to a removed webhook's URL answers 404 and wakes ",
        "nothing. A trigger already gone answers 204 as well.",
    ),
    params(
        afd_http::openapi::path::Fleet,
        ("trigger_id" = String, Path, description = "The trigger's identifier."),
    ),
    responses((status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = "Invalid identity"), (status = 404, description = "Fleet unavailable"),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn remove<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(TriggerPath {
        fleet_id,
        trigger_id,
    }): Path<TriggerPath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let trigger =
        Uuid7::parse(&trigger_id).map_err(|_invalid| Refusal::malformed(DETAIL_TRIGGER_ID))?;
    // Removed or already gone, the row is absent afterwards: a DELETE is
    // idempotent, so both answer 204.
    services
        .creations()
        .remove_trigger(access(&owned, &person), &fleet, &trigger)
        .await
        .map_err(setup_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
