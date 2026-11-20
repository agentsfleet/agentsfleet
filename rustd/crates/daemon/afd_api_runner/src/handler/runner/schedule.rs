//! `/v1/runners/me/runs/{run_id}/schedules` — a running fleet keeping its
//! own follow-ups on the daemon's schedule plane.
//!
//! # The fleet is the run's
//!
//! Every verb here proves the run first (`afd_fleet::run::Standing`) and
//! acts on the fleet that run runs. Nothing in a body or a query names a
//! fleet, so a runner cannot reach another fleet's schedules, and a holder a
//! reassignment has superseded is refused before any schedule is read.
//!
//! # A fleet reads every schedule and changes only its own
//!
//! The list shows a person's schedules beside the fleet's, each naming its
//! source, so the model plans around what exists. An edit or a delete of a
//! schedule a person made answers `UZ-SCHED-010`. The store, the reconcile to
//! `QStash` and every rendering are the tenant surface's own, so a schedule a
//! fleet made behaves exactly as one a person made.
//!
//! # What a fleet writes is masked
//!
//! A schedule's message is the prompt a later run reads, and it is stored and
//! handed to `QStash`. Every declared secret of the fleet is masked out of it
//! before either, as a message to a thread is.
//!
//! [`create`] and [`list`] are here, with the helpers the sibling verbs share;
//! the edit and the delete are in `schedule_edit`, the runs in `schedule_run`.

use std::sync::Arc;

use af_core::error_code::{self, ErrorCode};
use af_core::id::Uuid7;
use af_wire::schedule_verb::{QUERY_FENCING_TOKEN, ScheduleCreateRequest};
use afd_api_wire::schedule::Page;
use afd_cron::model::DEFAULT_TIMEZONE;
use afd_cron::{NewSchedule, Reconciled, Refused, Source, validate};
use afd_fleet::run::Standing;
use afd_fleet::run::write_fence::WriteFence;
use afd_http::handler::schedule::{checked, not_fleet_owned, not_found, refused, rendered};
use afd_http::handler::{Refusal, parameter, parse_id, read_strict_body};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::RunnerIdentity;
use crate::services::{FleetSchedules as _, RunPlane as _, Services};

/// The scoped event a schedules verb that failed is logged under.
pub(super) const EVENT_FAILED: &str = "runner_schedules_failed";

/// A fleet created a schedule.
const EVENT_CREATED: &str = "fleet_schedule_created";

/// A fleet's schedule write was refused for a bound, an owner or a field.
const EVENT_REFUSED: &str = "fleet_schedule_refused";

/// The refusal a body this verb cannot read earns.
pub(super) const DETAIL_MALFORMED: &str = "Malformed schedules body";

/// The refusal a run path segment that is not an identifier earns: one
/// spelling for every run-addressed verb.
pub(super) const DETAIL_RUN_ID: &str = "run_id must be a valid UUIDv7";

/// The refusal a schedule path segment that is not an identifier earns.
const DETAIL_SCHEDULE_ID: &str = "schedule_id must be a valid UUIDv7";

/// The refusal a read or a delete without its fence earns.
pub(super) const DETAIL_FENCE: &str = "fencing_token must be the run's fencing token";

/// The view a create and an edit answer with.
#[cfg(feature = "openapi")]
pub(super) const RECONCILED: &str = "The schedule as reconciled with the scheduler";

/// What [`create`] documents, its cap spelled from the constant.
#[cfg(feature = "openapi")]
const CREATE_DESCRIPTION: &str = const_format::concatcp!(
    "Creates a schedule for the assigned fleet and registers it with QStash. Source ",
    "is `fleet`; secret values are masked in its message. A fleet may create ",
    afd_cron::FLEET_SCHEDULES_MAX,
    " schedules; excess returns `UZ-SCHED-009`. `once` retires the schedule after its ",
    "first delivery. Registration failures still return 201 with `sync` state. Each ",
    "request creates another schedule; `Idempotency-Key` is unsupported.",
);

/// Creates a schedule for the fleet the run runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUN_SCHEDULES,
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_create_schedule",
    summary = "Create a schedule for the running fleet",
    description = CREATE_DESCRIPTION,
    request_body = ScheduleCreateRequest<'_>,
    params(afd_http::openapi::path::Run),
    responses(
        (status = 201, description = RECONCILED, body = afd_api_wire::schedule::View<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: ScheduleCreateRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let now = services.now();
    let run = standing(&*services, runner.id(), &run_id, request.fencing_token).await?;
    let timezone = request.timezone.as_deref().unwrap_or(DEFAULT_TIMEZONE);
    checked_for(
        &run,
        validate::Fields {
            expression: Some(&request.cron),
            timezone: Some(timezone),
            message: Some(&request.message),
        },
    )?;
    let message = masked(&*services, &run, &request.message).await?;
    let guard = write_fence(runner.id(), &run_id, request.fencing_token)?;
    let created = services
        .schedules()
        .create_guarded(
            &run.workspace_id,
            NewSchedule {
                fleet: &run.fleet_id,
                source: Source::Fleet,
                provider_schedule_id: None,
                cron: &request.cron,
                timezone,
                message: &message,
                once: request.once,
            },
            now,
            &guard,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let reconciled = created.map_err(refused_for(&run))?;
    if let Reconciled::Synced(schedule) | Reconciled::Failed(schedule) = &reconciled {
        log_written(&run, &schedule.schedule_id, EVENT_CREATED);
    }
    rendered(reconciled, StatusCode::CREATED, now)
}

/// Lists every schedule of the fleet the run runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = af_wire::paths::RUN_SCHEDULES,
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_list_schedules",
    summary = "List the running fleet's schedules",
    description = concat!(
        "Lists the assigned fleet's schedules oldest first. `source` is `api` for ",
        "user-created schedules, `trigger` for setup-defined schedules, or `fleet` for ",
        "fleet-created schedules. This runner can change only `fleet` schedules.",
    ),
    params(afd_http::openapi::path::Run, afd_http::openapi::query::Fence),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = Page<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let token = fence(query.as_deref())?;
    let run = standing(&*services, runner.id(), &run_id, token).await?;
    let schedules = services
        .schedules()
        .list(&run.workspace_id, &run.fleet_id)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let now = services.now();
    Ok(Json(Page {
        schedules: schedules
            .iter()
            .map(|schedule| schedule.view(now))
            .collect(),
    })
    .into_response())
}

/// Proves `run_id` is `runner`'s live run under `token`.
pub(super) async fn standing<D: Services>(
    services: &D,
    runner: &Uuid7,
    run_id: &str,
    token: u64,
) -> Result<Standing, Refusal> {
    services
        .runs()
        .standing(runner, run(run_id)?, token)
        .await
        .map_err(Refusal::at(EVENT_FAILED))
}

/// The run a write under `run_id` proves again, on the write's own
/// transaction: [`standing`] lets go, and a reassignment can land before the write.
pub(super) fn write_fence(runner: &Uuid7, run_id: &str, token: u64) -> Result<WriteFence, Refusal> {
    Ok(WriteFence::new(runner.clone(), run(run_id)?, token))
}

/// A refusal of a write the fleet asked for, logged against its run.
pub(super) fn refused_for(run: &Standing) -> impl FnOnce(Refused) -> Refusal + '_ {
    move |refusal| {
        log_refused(run, refusal.code());
        refused(refusal)
    }
}

/// The run a path names.
pub(super) fn run(raw: &str) -> Result<Uuid7, Refusal> {
    parse_id(raw, DETAIL_RUN_ID)
}

/// `fields` checked, a refusal logged against the run's fleet.
pub(super) fn checked_for(run: &Standing, fields: validate::Fields<'_>) -> Result<(), Refusal> {
    checked(fields).inspect_err(|_invalid| log_refused(run, error_code::INVALID_REQUEST))
}

/// `text` with the run's fleet's declared secrets masked.
pub(super) async fn masked<D: Services>(
    services: &D,
    run: &Standing,
    text: &str,
) -> Result<String, Refusal> {
    services
        .runs()
        .masked(run, text)
        .await
        .map_err(Refusal::at(EVENT_FAILED))
}

/// Logs a schedule the fleet wrote, under `event`; never its message.
pub(super) fn log_written(run: &Standing, schedule: &Uuid7, event: &'static str) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let fleet_id = run.fleet_id.as_str();
    let schedule_id = schedule.as_str();
    let agentsfleet_turn_id = run.turn_id.as_str();
    tracing::info!(fleet_id, schedule_id, agentsfleet_turn_id, event);
}

/// The schedule a path names.
pub(super) fn schedule_id(raw: &str) -> Result<Uuid7, Refusal> {
    parse_id(raw, DETAIL_SCHEDULE_ID)
}

/// The fence a read or a delete carries in its query.
pub(super) fn fence(query: Option<&str>) -> Result<u64, Refusal> {
    parameter(query.unwrap_or_default(), QUERY_FENCING_TOKEN)
        .and_then(|token| token.parse().ok())
        .ok_or_else(|| Refusal::malformed(DETAIL_FENCE))
}

/// Refuses unless `schedule` is the run's fleet's, and one the fleet made.
pub(super) async fn fleet_owned<D: Services>(
    services: &D,
    run: &Standing,
    schedule: &Uuid7,
) -> Result<(), Refusal> {
    let found = services
        .schedules()
        .one(&run.workspace_id, &run.fleet_id, schedule)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?
        .ok_or_else(not_found)?;
    if found.source == Source::Fleet {
        Ok(())
    } else {
        log_refused(run, error_code::SCHEDULE_NOT_FLEET_OWNED);
        Err(not_fleet_owned())
    }
}

/// Logs a refused schedule write by its code; never the body.
pub(super) fn log_refused(run: &Standing, code: ErrorCode) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let fleet_id = run.fleet_id.as_str();
    let error_code = code.as_str();
    tracing::info!(fleet_id, error_code, event = EVENT_REFUSED);
}
