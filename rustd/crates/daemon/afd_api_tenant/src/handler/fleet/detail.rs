//! Delete a fleet in any status; native reads and edits live in `native`.

use crate::auth::{PersonIdentity, WorkspaceContext};
use crate::handler::Refusal;
use crate::services::TenantPlane;
use afd_http::handler::setup_refusal;
pub(crate) use afd_http::handler::{FleetPath, parse_fleet_id};
use afd_http::services::FleetSchedules as _;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode};
use std::sync::Arc;

/// `DELETE /v1/workspaces/{workspace_id}/fleets/{fleet_id}` — purge a fleet in any status.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}",
    tag = afd_http::openapi::tag::FLEETS,
    operation_id = "delete_fleet",
    summary = "Permanently delete a fleet",
    description = concat!(
        "Permanently deletes the fleet, events, runs, approvals, grants, ",
        "keys, and memory, in any status. A running fleet's live work ends ",
        "first, and its schedules are removed from Upstash QStash before the ",
        "fleet goes. This cannot be undone. Archive is a separate ",
        "operation: set the status to `archived` to keep the history instead. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
        ("If-Match" = String, Header, description = "Required current resource tag"),
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = "A schedule is still registered with the scheduler; repeat the delete"),
        (status = 412, description = "Stale resource tag"),
        (status = 428, description = "Current resource tag required"),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn purge<D: TenantPlane>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    person: PersonIdentity,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    headers: HeaderMap,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let expected = crate::handler::input::expected(&headers)?.ok_or_else(|| {
        Refusal::coded(
            af_core::error_code::SETUP_PRECONDITION,
            "Send the current fleet If-Match tag",
        )
    })?;
    let access = afd_setup::Access {
        workspace: &owned.workspace,
        subject: person.subject(),
    };
    // One transaction holds the fleet's row from the tag check to the purge,
    // so a fleet change waits instead of refusing the delete after its
    // schedules are gone. The schedules go upstream on that transaction
    // first: the cascade reaches their rows, never the scheduler.
    let creations = services.creations();
    let mut held = creations
        .hold_delete(access, &fleet, expected)
        .await
        .map_err(setup_refusal)?;
    let released = services
        .schedules()
        .release_on(held.connection(), &owned.workspace, &fleet, services.now())
        .await
        .map_err(release_refusal)?;
    if let Err(registered) = released.registered() {
        // The removals that landed upstream lose their rows; the fleet stays
        // under the same tag, for the repeated delete.
        creations
            .keep(held, released.into_announcement())
            .await
            .map_err(setup_refusal)?;
        return Err(release_refusal(registered));
    }
    // The schedule receipts go with the fleet's history; the workspace's
    // walls hear the fleet set changed instead.
    drop(released);
    creations.delete(held).await.map_err(setup_refusal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

/// The event a fleet delete logs when its schedules would not go.
const EVENT_RELEASE: &str = "fleet_schedules_release_failed";

/// A schedule removal that stopped the fleet's delete, as the caller reads it:
/// a `409` naming the fleet's `deleting` state while a schedule is still
/// registered.
fn release_refusal(error: afd_cron::Error) -> Refusal {
    match error.current_state() {
        Some(state) => Refusal::conflict_at(EVENT_RELEASE, state)(error),
        None => Refusal::at(EVENT_RELEASE)(error),
    }
}
