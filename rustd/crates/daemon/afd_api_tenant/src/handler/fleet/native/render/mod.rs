//! A fleet as one read answers it: its row, its schedules, its trigger rows
//! with a webhook's address, and its grants.

use crate::{
    handler::Refusal,
    services::{FleetSchedules as _, TenantPlane},
};
use af_core::id::Uuid7;
use afd_api_wire::fleet::{CreatedFleet, FirstTurn, FirstTurnDisposition, FleetResponse};
use afd_api_wire::schedule::View;
use afd_setup::types::SavedFleet;
use axum::{
    Json,
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};

const EVENT_SCHEDULES: &str = "fleet_schedules_read_failed";

/// A fleet as the wire shows it, its schedules rendered against the clock.
type Shown<'s> = FleetResponse<'s, View<'s>>;

/// One read or change, as `status` with the fleet's tag.
pub(super) async fn response<D: TenantPlane>(
    services: &D,
    workspace: &Uuid7,
    saved: SavedFleet,
    status: StatusCode,
) -> Result<Response, Refusal> {
    let schedules = schedules(services, workspace, &saved.id).await?;
    let etag = saved.etag.clone();
    let payload = payload(services, &saved, &schedules);
    Ok((status, [(header::ETAG, etag)], Json(payload)).into_response())
}

/// A create: 201 with the fleet's address, or 200 for a repeat that made
/// nothing new, with the first turn either way.
pub(super) async fn created<D: TenantPlane>(
    services: &D,
    workspace: &Uuid7,
    created: afd_setup::types::CreatedFleet,
) -> Result<Response, Refusal> {
    let saved = &created.fleet;
    let schedules = schedules(services, workspace, &saved.id).await?;
    let location = format!("/v1/workspaces/{workspace}/fleets/{}", saved.id);
    let status = if created.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    let body = CreatedFleet {
        fleet: payload(services, saved, &schedules),
        // Nothing waits ahead of a fleet's first turn and no run is live, so,
        // as a steer with nothing ahead of it, it starts a run of its own.
        accepted: FirstTurn {
            turn_id: created.turn_id.to_string(),
            disposition: FirstTurnDisposition::Started,
        },
        replayed: created.replayed,
    };
    Ok((
        status,
        [
            (header::ETAG, saved.etag.clone()),
            (header::LOCATION, location),
        ],
        Json(body),
    )
        .into_response())
}

async fn schedules<D: TenantPlane>(
    services: &D,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<Vec<afd_cron::Schedule>, Refusal> {
    services
        .schedules()
        .list(workspace, fleet)
        .await
        .map_err(Refusal::at(EVENT_SCHEDULES))
}

fn payload<'s, D: TenantPlane>(
    services: &D,
    saved: &'s SavedFleet,
    schedules: &'s [afd_cron::Schedule],
) -> Shown<'s> {
    FleetResponse {
        id: saved.id.to_string(),
        name: saved.name.clone(),
        status: saved.status.clone(),
        etag: saved.etag.clone(),
        model_ref: saved.model_ref.clone(),
        http_egress: saved.http_egress,
        package_domains: saved.package_domains,
        schedules: crate::handler::schedule::views(schedules, services.now()),
        triggers: saved
            .triggers
            .iter()
            .map(|row| row.shown(services.deployment(), &saved.id))
            .collect(),
        grants: saved
            .grants
            .iter()
            .map(crate::handler::grant::shown)
            .collect(),
        memory_access: afd_api_wire::fleet::MemoryAccess {
            read: saved.memory_reads_workspace,
            publish: saved.memory_publishes_workspace,
        },
        created_at: saved.created_at,
        updated_at: saved.updated_at,
        events_processed: saved.events_processed.clone(),
        budget_used_nanos: saved.budget_used_nanos.clone(),
        pending_approvals: saved.pending_approvals,
    }
}
