//! The four verbs that reconcile a row against the external scheduler.
//!
//! Every one of them writes locally and pushes upstream in the same request,
//! and the answer carries the sync state rather than the outcome of the push —
//! the module note on [`super`] argues why a scheduler that refused is still a
//! 201. Validation runs before the row is written, so a refused expression
//! leaves neither a row nor a registration for somebody to clear.

use std::sync::Arc;

use afd_core::error_code;
use afd_cron::{Change, DesiredStatus, NewSchedule, Refused, validate};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::Response;
use http::StatusCode;

use crate::handler::Refusal;
use crate::handler::fleet::detail::parse_fleet_id;
use crate::services::{FleetSchedules as _, Services};

use super::support::{Create, Patch, checked, held_or, rendered};
use super::{
    DETAIL_DUPLICATE, DETAIL_INVALID_BODY, DETAIL_INVALID_CRON, DETAIL_INVALID_MESSAGE,
    DETAIL_INVALID_TIMEZONE, DETAIL_NOT_FOUND, DETAIL_TOO_MANY, EVENT_WRITE,
};

/// `POST …/schedules`.
///
/// # Errors
/// `UZ-REQ-002` for a body or a field this daemon will not register, and the
/// ceiling and duplicate refusals the store answers.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "create_fleet_schedule",
    summary = "Create a hosted schedule",
    description = "Creates a schedule and synchronously registers it in Upstash QStash. ",
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 422, description = afd_http::openapi::UNPROCESSABLE),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    Path((workspace_id, fleet_id)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let workspace = parse_fleet_id(&workspace_id)?;

    let input: Create = serde_json::from_slice(&body)
        .map_err(|_unreadable| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_INVALID_BODY))?;
    let timezone = input
        .timezone
        .unwrap_or_else(|| afd_cron::model::DEFAULT_TIMEZONE.to_owned());

    // Validated BEFORE anything is written, so a refused schedule leaves no row
    // and no upstream call — an expression the scheduler would reject must not
    // reach it, because a failed registration is a state somebody has to clear.
    checked(validate::cron(&input.cron), DETAIL_INVALID_CRON)?;
    checked(validate::timezone(&timezone), DETAIL_INVALID_TIMEZONE)?;
    checked(validate::message(&input.message), DETAIL_INVALID_MESSAGE)?;

    let created = services
        .schedules()
        .create(
            &workspace,
            NewSchedule {
                fleet: &fleet,
                source: afd_cron::Source::Api,
                // The upstream key is this daemon's own name for the schedule
                // until the scheduler answers with its id. It has to be unique
                // per fleet, which the fleet's own identifier plus the instant
                // already is.
                source_key: &format!("{fleet_id}-{}", services.now().as_millis()),
                cron: &input.cron,
                timezone: &timezone,
                message: &input.message,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    match created {
        Err(Refused::NoSuchFleet) => Err(Refusal::coded(
            error_code::SCHEDULE_NOT_FOUND,
            DETAIL_NOT_FOUND,
        )),
        Err(Refused::TooMany) => Err(Refusal::coded(
            error_code::SCHEDULE_LIMIT_REACHED,
            DETAIL_TOO_MANY,
        )),
        Err(Refused::DuplicateKey) => Err(Refusal::coded(
            error_code::SCHEDULE_KEY_TAKEN,
            DETAIL_DUPLICATE,
        )),
        Ok(reconciled) => rendered(reconciled, StatusCode::CREATED),
    }
}

/// `PATCH …/schedules/{schedule_id}`.
///
/// # Errors
/// As [`create`], plus `UZ-REQ-004` for a schedule this fleet does not hold.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "update_fleet_schedule",
    summary = "Update a hosted schedule",
    description = concat!(
        "Updates schedule fields and synchronously overwrites the QStash ",
        "registration. ",
    ),
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 422, description = afd_http::openapi::UNPROCESSABLE),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    Path((_workspace, fleet_id, schedule_id)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let input: Patch = serde_json::from_slice(&body)
        .map_err(|_unreadable| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_INVALID_BODY))?;

    if let Some(cron) = input.cron.as_deref() {
        checked(validate::cron(cron), DETAIL_INVALID_CRON)?;
    }
    if let Some(timezone) = input.timezone.as_deref() {
        checked(validate::timezone(timezone), DETAIL_INVALID_TIMEZONE)?;
    }
    if let Some(message) = input.message.as_deref() {
        checked(validate::message(message), DETAIL_INVALID_MESSAGE)?;
    }

    let changed = services
        .schedules()
        .change(
            &fleet,
            &schedule,
            Change {
                cron: input.cron.as_deref(),
                timezone: input.timezone.as_deref(),
                message: input.message.as_deref(),
                desired_status: input.paused.map(|paused| {
                    if paused {
                        DesiredStatus::Paused
                    } else {
                        DesiredStatus::Active
                    }
                }),
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    held_or(changed, StatusCode::OK)
}

/// `DELETE …/schedules/{schedule_id}` — see the module note on why it does not.
///
/// # Errors
/// As [`patch`].
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "delete_fleet_schedule",
    summary = "Delete a hosted schedule",
    description = concat!(
        "Deletes the QStash registration before removing the local schedule ",
        "row. ",
    ),
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn purge<D: Services>(
    State(services): State<Arc<D>>,
    Path((_workspace, fleet_id, schedule_id)): Path<(String, String, String)>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let removed = services
        .schedules()
        .change(
            &fleet,
            &schedule,
            Change {
                desired_status: Some(DesiredStatus::Deleting),
                ..Change::default()
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    held_or(removed, StatusCode::OK)
}

/// `POST …/schedules/{schedule_id}/sync`.
///
/// `/sync` rather than the Zig's `:sync` — a published-surface divergence the
/// router forces, argued at [`crate::route::FleetRoute::ScheduleSync`].
///
/// # Errors
/// As [`patch`].
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}/sync",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "sync_fleet_schedule",
    summary = "Sync a hosted schedule",
    description = concat!(
        "Idempotently overwrites the QStash registration from the latest ",
        "local generation. ",
    ),
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn sync<D: Services>(
    State(services): State<Arc<D>>,
    Path((_workspace, fleet_id, schedule_id)): Path<(String, String, String)>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let synced = services
        .schedules()
        .sync(&fleet, &schedule, services.now())
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    held_or(synced, StatusCode::OK)
}
