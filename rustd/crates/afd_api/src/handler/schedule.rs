//! `/v1/workspaces/{id}/fleets/{fleet_id}/schedules` — the CRUD half of §3.
//!
//! # A create answers 201 even when the scheduler refused
//!
//! The row is saved either way, and the answer carries the sync state so the
//! caller can see which happened. Answering 502 for an upstream refusal would
//! be telling a person their schedule was not created when it was — and the
//! next `:sync` would then repair a schedule they believe does not exist.
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
use serde::{Deserialize, Serialize};

use crate::handler::Refusal;
use crate::handler::fleet::detail::parse_fleet_id;
use crate::services::{FleetSchedules as _, Services};

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

/// One schedule, as this surface renders it.
#[derive(Debug, Serialize)]
struct View<'s> {
    /// Its identity.
    schedule_id: &'s str,
    /// The fleet it wakes.
    fleet_id: &'s str,
    /// The expression it fires on.
    cron: &'s str,
    /// The zone that expression is read in.
    timezone: &'s str,
    /// What the fleet is asked to do.
    message: &'s str,
    /// What the operator wants it to be doing.
    status: &'s str,
    /// How far the external scheduler has been brought in line.
    ///
    /// Rendered rather than hidden: a schedule that saved and did not register
    /// is the one state a person needs to see, and a view that showed only the
    /// intent would report a schedule as live when it fires nowhere.
    sync: &'s str,
    /// Why the last push failed, when one did.
    last_error: Option<&'s str>,
    /// When it was created.
    created_at: i64,
    /// When it was last changed.
    updated_at: i64,
}

impl<'s> View<'s> {
    /// One row, rendered.
    fn of(schedule: &'s Schedule) -> Self {
        Self {
            schedule_id: schedule.schedule_id.as_str(),
            fleet_id: schedule.fleet_id.as_str(),
            cron: &schedule.cron,
            timezone: &schedule.timezone,
            message: &schedule.message,
            status: schedule.desired_status.as_str(),
            sync: schedule.sync_status.as_str(),
            last_error: schedule.last_error.as_deref(),
            created_at: schedule.created_at,
            updated_at: schedule.updated_at,
        }
    }
}

/// A page of schedules.
#[derive(Debug, Serialize)]
struct Page<'s> {
    /// The fleet's schedules, oldest first.
    schedules: Vec<View<'s>>,
}

/// `GET …/schedules`.
///
/// # Errors
/// Reports a datastore that would not answer.
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
        schedules: schedules.iter().map(View::of).collect(),
    })
    .into_response())
}

/// `POST …/schedules`.
///
/// # Errors
/// `UZ-REQ-002` for a body or a field this daemon will not register, and the
/// ceiling and duplicate refusals the store answers.
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

/// `POST …/schedules/{schedule_id}:sync`.
///
/// # Errors
/// As [`patch`].
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
            Ok((status, Json(View::of(&schedule))).into_response())
        }
        Reconciled::Removed => Ok(StatusCode::NO_CONTENT.into_response()),
        Reconciled::Superseded => Err(Refusal::coded(error_code::SCHEDULE_SYNCING, DETAIL_HELD)),
    }
}
