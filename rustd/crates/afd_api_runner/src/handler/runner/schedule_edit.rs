//! `PATCH|DELETE /v1/runners/me/leases/{lease_id}/schedules/{schedule_id}` —
//! a fleet changing or retiring a schedule it made.
//!
//! Both prove the lease, then that the schedule is the fleet's and was made by
//! it (`super::schedule::fleet_owned`), then reconcile exactly as the tenant
//! surface does. A delete does not delete: it sets `deleting` and pushes, and
//! the row goes once `QStash` agrees — see `afd_cron::DesiredStatus::Deleting`.

use std::sync::Arc;

use afd_cron::{Change, DesiredStatus, Reconciled, validate};
use afd_http::handler::schedule::held_or;
use afd_http::handler::{Refusal, read_strict_body};
use afd_wire::schedule_verb::SchedulePatchRequest;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::Response;
use http::StatusCode;

use super::schedule::{
    DETAIL_MALFORMED, EVENT_FAILED, checked_for, fence, fleet_owned, log_written, masked,
    schedule_id, standing,
};
use crate::auth::RunnerIdentity;
use crate::services::{FleetSchedules as _, Services};

/// A fleet retired one of its schedules.
const EVENT_DELETED: &str = "fleet_schedule_deleted";

/// Changes a schedule the fleet made.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_update_schedule",
    summary = "Change a schedule the fleet made",
    description = concat!(
        "Changes the fields a body names on a schedule the running fleet ",
        "created, and pushes the result to QStash. A new message has the ",
        "fleet's declared secrets masked out. A schedule a person made ",
        "answers `UZ-SCHED-010`. Sending the same body twice leaves the same ",
        "schedule. ",
    ),
    request_body = SchedulePatchRequest,
    params(afd_http::openapi::path::LeaseSchedule),
    responses(
        (status = 200, description = super::schedule::RECONCILED, body = afd_wire::schedule::View),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn update<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease_id, schedule_id_raw)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: SchedulePatchRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let schedule = schedule_id(&schedule_id_raw)?;
    let now = services.now();
    let lease = standing(
        &*services,
        runner.id(),
        &lease_id,
        request.fencing_token,
        now,
    )
    .await?;
    checked_for(
        &lease,
        validate::Fields {
            expression: request.cron.as_deref(),
            timezone: request.timezone.as_deref(),
            message: request.message.as_deref(),
        },
    )?;
    fleet_owned(&*services, &lease, &schedule).await?;
    let message = match request.message.as_deref() {
        Some(text) => Some(masked(&*services, &lease, text).await?),
        None => None,
    };
    let changed = services
        .schedules()
        .change(
            &lease.fleet_id,
            &schedule,
            Change {
                cron: request.cron.as_deref(),
                timezone: request.timezone.as_deref(),
                message: message.as_deref(),
                desired_status: request.paused.map(DesiredStatus::of_paused),
            },
            now,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    held_or(changed, StatusCode::OK)
}

/// Retires a schedule the fleet made.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_delete_schedule",
    summary = "Delete a schedule the fleet made",
    description = concat!(
        "Removes a schedule the running fleet created from QStash, then ",
        "deletes the row. It answers 204 once QStash agrees, or 200 with the ",
        "schedule until then. A schedule a person made answers `UZ-SCHED-010`. ",
    ),
    params(afd_http::openapi::path::LeaseSchedule, afd_http::openapi::query::Fence),
    responses(
        (status = 200, description = super::schedule::RECONCILED, body = afd_wire::schedule::View),
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn remove<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease_id, schedule_id_raw)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let schedule = schedule_id(&schedule_id_raw)?;
    let token = fence(query.as_deref())?;
    let now = services.now();
    let lease = standing(&*services, runner.id(), &lease_id, token, now).await?;
    fleet_owned(&*services, &lease, &schedule).await?;
    let removed = services
        .schedules()
        .change(
            &lease.fleet_id,
            &schedule,
            Change {
                desired_status: Some(DesiredStatus::Deleting),
                ..Change::default()
            },
            now,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    // Logged once the row is gone: a delete QStash has not yet agreed to
    // answers the row, and a superseded one a conflict.
    if matches!(removed, Some(Reconciled::Removed)) {
        log_written(&lease, &schedule, EVENT_DELETED);
    }
    held_or(removed, StatusCode::OK)
}
