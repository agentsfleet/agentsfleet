//! The four verbs that reconcile a row against the external scheduler.
//!
//! Every one of them writes locally and pushes upstream in the same request,
//! and the answer carries the sync state rather than the outcome of the push —
//! the module note on [`super`] argues why a scheduler that refused is still a
//! 201. Validation runs before the row is written, so a refused expression
//! leaves neither a row nor a registration for somebody to clear.

use std::sync::Arc;

use afd_api_wire::schedule::Patch;
use afd_core::error_code;
use afd_cron::{Change, DesiredStatus, NewSchedule, validate};
use afd_http::handler::schedule::{checked, held_or, refused, rendered};
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::Response;
use http::{HeaderMap, StatusCode};

use crate::auth::WorkspaceContext;
use crate::handler::Refusal;
use crate::handler::fleet::detail::{FleetPath, parse_fleet_id};
use crate::services::{FleetSchedules as _, Services};

use super::support::{Create, SchedulePath};
use super::{DETAIL_INVALID_BODY, EVENT_WRITE};

/// What a create and an update both answer: the row as the scheduler now holds it.
// Read only by the two annotations, which the default build compiles away.
#[cfg(feature = "openapi")]
const RECONCILED: &str = "Saved schedule with its registration status";

/// `POST …/schedules`.
///
/// # Errors
/// `UZ-REQ-001` for a body or a field this daemon will not register, and the
/// ceiling and duplicate refusals the store answers.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "create_fleet_schedule",
    summary = "Create a hosted schedule",
    description = concat!(
        "Saves a schedule and attempts registration with Upstash QStash. A 201 response ",
        "can still contain a registration failure. Check `sync`, `last_error`, and ",
        "`run_state`; use this schedule's `/sync` endpoint to retry registration.",
    ),
    request_body = Create,
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 201, description = RECONCILED, body = afd_api_wire::schedule::View),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 422, description = afd_http::openapi::UNPROCESSABLE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(FleetPath { fleet_id }): Path<FleetPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;

    let input: Create = serde_json::from_slice(&body)
        .map_err(|_unreadable| Refusal::coded(error_code::INVALID_REQUEST, DETAIL_INVALID_BODY))?;
    let timezone = input
        .timezone
        .unwrap_or_else(|| afd_cron::model::DEFAULT_TIMEZONE.to_owned());

    // Validated BEFORE anything is written, so a refused schedule leaves no row
    // and no upstream call — an expression the scheduler would reject must not
    // reach it, because a failed registration is a state somebody has to clear.
    checked(validate::Fields {
        expression: Some(&input.cron),
        timezone: Some(&timezone),
        message: Some(&input.message),
    })?;

    let created = services
        .schedules()
        .create(
            &owned.workspace,
            NewSchedule {
                fleet: &fleet,
                source: afd_cron::Source::Api,
                provider_schedule_id: None,
                cron: &input.cron,
                timezone: &timezone,
                message: &input.message,
                once: false,
            },
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    rendered(
        created.map_err(refused)?,
        StatusCode::CREATED,
        services.now(),
    )
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
        "Saves schedule changes and attempts to update the Upstash QStash registration. ",
        "A 200 response can still contain a registration failure. Check `sync`, ",
        "`last_error`, and `run_state`; use this schedule's `/sync` endpoint to retry registration.",
    ),
    request_body = Patch,
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 200, description = RECONCILED, body = afd_api_wire::schedule::View),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 412, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 422, description = afd_http::openapi::UNPROCESSABLE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn patch<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(SchedulePath {
        fleet_id,
        schedule_id,
    }): Path<SchedulePath>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;
    let input: Patch = crate::handler::input::body(&body)?;
    // The fields a patch supplies are refused for what they say before the row
    // is looked up, so an unusable expression never reads as a missing row.
    // The store validates the merged result again once it knows the row.
    checked(validate::Fields {
        expression: input.cron.as_deref(),
        timezone: input.timezone.as_deref(),
        message: input.message.as_deref(),
    })?;
    let expected = crate::handler::input::expected(&headers)?;
    let changed = services
        .schedules()
        .edit(
            afd_cron::EditRequest {
                workspace: &owned.workspace,
                fleet: &fleet,
                schedule: &schedule,
                expected,
                patch: &input,
            },
            services.now(),
        )
        .await
        .map_err(afd_http::handler::schedule::edit_refusal)?;
    rendered(changed, StatusCode::OK, services.now())
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
    description = "Removes the schedule from Upstash QStash before deleting its saved record.",
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
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
pub(crate) async fn purge<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(SchedulePath {
        fleet_id,
        schedule_id,
    }): Path<SchedulePath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let removed = services
        .schedules()
        .change(
            &owned.workspace,
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

    held_or(removed, StatusCode::OK, services.now())
}

/// `POST …/schedules/{schedule_id}/sync`.
///
/// `/sync` rather than a `:sync` verb suffix — the router forces the segment,
/// argued at [`crate::route::FleetRoute::ScheduleSync`].
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
        "Replaces the Upstash QStash registration with the latest saved schedule. Repeating ",
        "this request is safe.",
    ),
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 200, description = "The schedule as re-reconciled with the scheduler", body = afd_api_wire::schedule::View),
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
pub(crate) async fn sync<D: Services>(
    State(services): State<Arc<D>>,
    WorkspaceContext(owned): WorkspaceContext,
    Path(SchedulePath {
        fleet_id,
        schedule_id,
    }): Path<SchedulePath>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let synced = services
        .schedules()
        .sync(&owned.workspace, &fleet, &schedule, services.now())
        .await
        .map_err(Refusal::at(EVENT_WRITE))?;

    held_or(synced, StatusCode::OK, services.now())
}
