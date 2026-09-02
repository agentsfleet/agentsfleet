//! `/v1/workspaces/{id}/fleets/{fleet_id}/schedules` — the CRUD half of §3.
//!
//! # A create answers 201 even when the scheduler refused
//!
//! The row is saved either way, and the answer carries the sync state so the
//! caller can see which happened. Answering 502 for an upstream refusal would
//! be telling a person their schedule was not created when it was — and the
//! next sync would then repair a schedule they believe does not exist.
//!
//! # A delete does not delete
//!
//! It sets `desired_status = deleting` and pushes. The row goes only once the
//! external scheduler has confirmed, because a row removed first would leave a
//! schedule firing at a callback this daemon can no longer resolve to a fleet —
//! see [`afd_cron::DesiredStatus::Deleting`].

use std::sync::Arc;

use afd_core::error_code;
use afd_cron::{Change, DesiredStatus, NewSchedule, Reconciled, Refused, Schedule, validate};
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use axum::{Json, body::Bytes};
use http::StatusCode;
use serde::Deserialize;

use crate::handler::Refusal;
use crate::handler::fleet::detail::parse_fleet_id;
use crate::services::{FleetSchedules as _, Services};
/// How this surface renders a schedule. Public wire: the dashboard and the
/// CLI both read it, so `afd_wire` owns the shape.
use afd_wire::schedule::{Page, View};

/// The scoped event a failed schedule read is logged under.
const EVENT_READ: &str = "schedule_read_failed";

/// The scoped event a failed schedule write is logged under.
const EVENT_WRITE: &str = "schedule_write_failed";

/// The refusal a body this route cannot read as a schedule earns.
const DETAIL_INVALID_BODY: &str = "The request body is not a schedule this daemon can read.";

/// The refusal an expression this daemon will not register earns.
const DETAIL_INVALID_CRON: &str =
    "The cron expression must be five numeric fields this daemon accepts.";

/// The refusal a zone this daemon will not pass upstream earns.
const DETAIL_INVALID_TIMEZONE: &str = "The timezone is not a name this daemon will register.";

/// The refusal a message that would wake a fleet with nothing earns.
const DETAIL_INVALID_MESSAGE: &str = "The message must not be empty.";

/// The refusal a fleet at its schedule ceiling earns.
const DETAIL_TOO_MANY: &str = "This fleet already holds as many schedules as it may.";

/// The refusal a duplicate upstream key earns.
const DETAIL_DUPLICATE: &str = "This fleet already has a schedule under that key.";

/// The refusal a schedule this fleet does not hold earns.
const DETAIL_NOT_FOUND: &str = "No schedule with that identifier belongs to this fleet.";

/// The refusal a schedule another syncer is holding earns.
///
/// A conflict rather than a not-found, because the row EXISTS and the caller
/// may retry in a moment — the two answers send a caller to different places.
const DETAIL_HELD: &str = "This schedule is being synchronised. Try again in a moment.";

/// What a caller sends to create a schedule.
#[derive(Debug, Deserialize)]
struct Create {
    /// The expression it fires on.
    cron: String,
    /// The zone that expression is read in.
    ///
    /// Absent means [`afd_cron::model::DEFAULT_TIMEZONE`] — a schedule with no
    /// stated zone is not an error, it is one written by somebody who did not
    /// think about zones, and UTC is the answer that surprises them least.
    timezone: Option<String>,
    /// What the fleet is asked to do when it fires.
    message: String,
}

/// What a caller sends to change one.
///
/// Every field optional, and an absent one is left alone — see
/// [`afd_cron::Change`] on why a partial edit is not a whole replacement.
#[derive(Debug, Deserialize)]
struct Patch {
    /// A new expression.
    cron: Option<String>,
    /// A new zone.
    timezone: Option<String>,
    /// A new message.
    message: Option<String>,
    /// Whether it should be firing.
    paused: Option<bool>,
}

/// One row, rendered.
///
/// A free function rather than an inherent `impl`: [`View`] is `afd_wire`'s
/// type now, and the rendering is this plane's knowledge of its own store.
fn view_of(schedule: &Schedule) -> View<'_> {
    View {
        schedule_id: schedule.schedule_id.as_str().into(),
        fleet_id: schedule.fleet_id.as_str().into(),
        cron: schedule.cron.as_str().into(),
        timezone: schedule.timezone.as_str().into(),
        message: schedule.message.as_str().into(),
        status: schedule.desired_status.as_str().into(),
        sync: schedule.sync_status.as_str().into(),
        last_error: schedule.last_error.as_deref().map(Into::into),
        created_at: schedule.created_at,
        updated_at: schedule.updated_at,
    }
}

/// `GET …/schedules`.
///
/// # Errors
/// Reports a datastore that would not answer.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "list_fleet_schedules",
    summary = "List hosted schedules",
    description = concat!(
        "Lists the hosted schedules for a Fleet. The result is bounded by the ",
        "per-Fleet schedule cap. ",
    ),
    params(
        afd_http::openapi::path::Fleet,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = Page),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    Path((_workspace, fleet_id)): Path<(String, String)>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedules = services
        .schedules()
        .list(&fleet)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    Ok(Json(Page {
        schedules: schedules.iter().map(view_of).collect(),
    })
    .into_response())
}

/// `GET …/schedules/{schedule_id}`.
///
/// The read half of the CRUD this section declares, and the one verb of it that
/// did not come across in the port: `public/openapi.json` declares this GET and
/// the published navigation lists it, while the router mounted only PATCH and
/// DELETE — so a client following the documented API earned a 405. The Zig
/// serves it at `handlers/schedules/api.zig:68`, and the service seam here
/// already carried [`FleetSchedules::one`]; only the handler and its mount were
/// missing.
///
/// A schedule belonging to another fleet answers exactly as one that never
/// existed. Telling them apart would confirm a schedule id across a fleet
/// boundary, which is the same reason [`FleetSchedules::one`] returns
/// `Ok(None)` for both.
///
/// # Errors
/// Reports a datastore that would not answer, and `UZ-SCHED-002` for a schedule
/// this fleet does not hold.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/schedules/{schedule_id}",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "get_fleet_schedule",
    summary = "Get a hosted schedule",
    description = "One schedule, as it is currently registered for this fleet.",
    params(
        afd_http::openapi::path::Schedule,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = View),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn one<D: Services>(
    State(services): State<Arc<D>>,
    Path((_workspace, fleet_id, schedule_id)): Path<(String, String, String)>,
) -> Result<Response, Refusal> {
    let fleet = parse_fleet_id(&fleet_id)?;
    let schedule = parse_fleet_id(&schedule_id)?;

    let found = services
        .schedules()
        .one(&fleet, &schedule)
        .await
        .map_err(Refusal::at(EVENT_READ))?;

    found.map_or_else(
        || {
            Err(Refusal::coded(
                error_code::SCHEDULE_NOT_FOUND,
                DETAIL_NOT_FOUND,
            ))
        },
        |schedule| Ok(Json(view_of(&schedule)).into_response()),
    )
}

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

/// One validation verdict, as a refusal.
fn checked(verdict: Result<(), validate::Invalid>, detail: &'static str) -> Result<(), Refusal> {
    verdict.map_err(|_invalid| Refusal::coded(error_code::INVALID_REQUEST, detail))
}

/// A reconcile that may have found no row, rendered.
fn held_or(reconciled: Option<Reconciled>, status: StatusCode) -> Result<Response, Refusal> {
    reconciled.map_or_else(
        || {
            Err(Refusal::coded(
                error_code::SCHEDULE_NOT_FOUND,
                DETAIL_NOT_FOUND,
            ))
        },
        |reconciled| rendered(reconciled, status),
    )
}

/// What one reconcile answers.
///
/// A superseded attempt is a 409 and not a 500: another syncer holds the row,
/// which is a real state a caller retries out of rather than an incident.
fn rendered(reconciled: Reconciled, status: StatusCode) -> Result<Response, Refusal> {
    match reconciled {
        Reconciled::Synced(schedule) | Reconciled::Failed(schedule) => {
            Ok((status, Json(view_of(&schedule))).into_response())
        }
        Reconciled::Removed => Ok(StatusCode::NO_CONTENT.into_response()),
        Reconciled::Superseded => Err(Refusal::coded(error_code::SCHEDULE_SYNCING, DETAIL_HELD)),
    }
}
