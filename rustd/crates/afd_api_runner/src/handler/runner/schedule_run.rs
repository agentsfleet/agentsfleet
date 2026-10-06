//! `/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs` — one
//! schedule's runs: the ones that ran, and a new one to fire it now.
//!
//! A run is an event whose actor is `cron:<schedule_id>`, which a `QStash` fire
//! and a run-now both record (`afd_cron::schedule_actor`). Running now admits
//! through the same producer `QStash`'s callback does, keyed by the event the
//! lease runs, so a retried post, from this lease or one a reclaim handed the
//! event to, answers the run it already created rather than a second one. A
//! run joins this list once a runner has taken it: until then it is queued
//! work, not history.
//!
//! # A run-now fires only what would fire
//!
//! The `QStash` callback drops a fire for a paused or retiring schedule and for
//! a fleet that will not take work, and a run-now answers those with a 409
//! rather than overriding them. A run a schedule started may not run one now
//! either: without that, each run could wake the next, and a fleet steered by
//! what it read could fan its own queue out without end.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::error_code;
use afd_core::paging::{CEILING, PagingRefusal, QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_cron::{ACTOR_PREFIX, FireTarget, schedule_actor};
use afd_events::{Cursor, EventRow, next_cursor};
use afd_fleet::lease::Standing;
use afd_fleet_lifecycle::FleetStatus;
use afd_http::handler::schedule::not_found;
use afd_http::handler::{Refusal, parameter, read_strict_body};
use afd_validate::Limit;
use afd_wire::event::EventsResponse;
use afd_wire::schedule_verb::{ScheduleRun, ScheduleRunRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use super::schedule::{DETAIL_MALFORMED, EVENT_FAILED, fence, log_refused, schedule_id, standing};
use crate::auth::RunnerIdentity;
use crate::services::{FleetSchedules as _, Services, WorkspaceEvents as _};

/// The refusal a page size outside the served band earns: the paging
/// vocabulary's own sentence, which names [`CEILING`].
const DETAIL_LIMIT: &str = PagingRefusal::Limit.detail();

/// The refusal a schedule that would not fire on its own earns.
const DETAIL_NOT_RUNNABLE: &str =
    "This schedule is paused or being removed, so it does not run now; a person resumes it";

/// The refusal a run a schedule started earns when it runs one now.
const DETAIL_SCHEDULED_RUN: &str =
    "A run a schedule started cannot run a schedule now, so no schedule wakes its fleet in a loop";

/// The refusal a fleet that will not take work earns.
const DETAIL_FLEET_NOT_RUNNABLE: &str = "The fleet is not taking new work, so nothing runs now";

/// The `current_state` a run a schedule started names.
const STATE_SCHEDULED_RUN: &str = "scheduled_run";

/// The `current_state` a fleet whose stored status this build cannot read
/// names: refused as not runnable, as the `QStash` callback drops it.
const STATE_FLEET_UNREADABLE: &str = "unknown";

/// The refusal a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "starting_after must be a next_cursor this daemon returned";

/// What a run-now's admission key names after the schedule: the event the
/// asking lease runs, so every retry of one run is one run, across a reclaim.
const RUN_KEY_PREFIX: &str = "run:";

/// Fires a schedule of the fleet's now.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_run_schedule",
    summary = "Run a schedule now",
    description = concat!(
        "Creates a run of a schedule of the running fleet now, as if QStash ",
        "had fired it. The run is one event with actor `cron:<schedule_id>`, ",
        "queued for the fleet. Posting again for the same leased event answers ",
        "the same run. A paused schedule, or one being removed, answers ",
        "`UZ-SCHED-011` with its `current_state`. A run a schedule started ",
        "answers the same with `scheduled_run`, so no schedule wakes its fleet ",
        "in a loop. A fleet that is not taking work answers `UZ-AGT-012`. A ",
        "`once` schedule fires once: a run-now that races its scheduled fire ",
        "answers that same run, and a repeat after it retires answers 404. ",
    ),
    request_body = ScheduleRunRequest,
    params(afd_http::openapi::path::LeaseSchedule),
    responses(
        (status = 201, description = "The run that was created", body = ScheduleRun),
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
pub(crate) async fn run<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease_id, schedule_id_raw)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: ScheduleRunRequest =
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
    // Read by the schedule alone, so the fleet is checked here: a schedule of
    // another fleet answers exactly as one that never existed.
    let target = services
        .schedules()
        .fire_target(&schedule)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?
        .filter(|target| target.fleet == lease.fleet_id)
        .ok_or_else(not_found)?;
    runnable(&lease, &target)?;
    let key = format!("{RUN_KEY_PREFIX}{}", lease.event_id);
    let fired = services
        .schedules()
        .fire(&schedule, &target, &key, now)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok((
        StatusCode::CREATED,
        Json(ScheduleRun {
            event_id: Cow::Owned(fired.event_id),
        }),
    )
        .into_response())
}

/// Lists one schedule's runs, newest first.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_list_schedule_runs",
    summary = "List a schedule's runs",
    description = concat!(
        "Lists the events a schedule of the running fleet fired, newest first: ",
        "every event with actor `cron:<schedule_id>` that a runner has taken. ",
        "A `once` schedule's runs stay listed after it retires. Page with ",
        "`starting_after` and `limit`. ",
    ),
    params(afd_http::openapi::path::LeaseSchedule, afd_http::openapi::query::ScheduleRunsPage),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = EventsResponse),
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
pub(crate) async fn runs<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease_id, schedule_id_raw)): Path<(String, String)>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let query = query.unwrap_or_default();
    let schedule = schedule_id(&schedule_id_raw)?;
    let token = fence(Some(&query))?;
    // The parameter names and the ceiling are `afd_core::paging`'s; the cursor
    // is the event store's own, because a run IS a history row and only that
    // store mints and reads its `next_cursor`. `Page::parse` decodes the core
    // cursor form, which no history page hands out.
    let limit = Limit::parse(parameter(&query, QUERY_LIMIT), CEILING)
        .map_err(|_break| Refusal::malformed(DETAIL_LIMIT))?;
    let cursor = parameter(&query, QUERY_STARTING_AFTER)
        .map(Cursor::decode)
        .transpose()
        .map_err(|_unminted| Refusal::malformed(DETAIL_CURSOR))?;
    let now = services.now();
    let lease = standing(&*services, runner.id(), &lease_id, token, now).await?;
    let rows = i64::from(limit);
    let page = services
        .events()
        .page_of_actor(
            &lease.workspace_id,
            &lease.fleet_id,
            &schedule_actor(&schedule),
            cursor.as_ref(),
            rows,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    // History outlives the row: a `once` schedule retires when it fires, and
    // its runs stay readable. Only an empty first page asks whether the
    // schedule exists, so an unknown or foreign id still answers 404.
    if page.is_empty() && cursor.is_none() {
        services
            .schedules()
            .one(&lease.fleet_id, &schedule)
            .await
            .map_err(Refusal::at(EVENT_FAILED))?
            .ok_or_else(not_found)?;
    }
    Ok(Json(EventsResponse {
        items: page.iter().map(EventRow::summary).collect(),
        next_cursor: next_cursor(&page, rows).map(|after| Cow::Owned(after.encode())),
    })
    .into_response())
}

/// Refuses a run-now the scheduler's own fire would not make, or one a
/// schedule's run asked for.
///
/// The fleet first, as the `QStash` callback orders it: a stopped fleet halts
/// everything, whatever its schedules say.
fn runnable(lease: &Standing, target: &FireTarget) -> Result<(), Refusal> {
    let fleet = FleetStatus::parse(&target.fleet_status);
    if !fleet.is_some_and(FleetStatus::is_runnable) {
        log_refused(lease, error_code::AGENTSFLEET_PAUSED_INGRESS);
        return Err(Refusal::conflict(
            error_code::AGENTSFLEET_PAUSED_INGRESS,
            DETAIL_FLEET_NOT_RUNNABLE,
            fleet.map_or(STATE_FLEET_UNREADABLE, FleetStatus::as_str),
        ));
    }
    if !target.desired_status.fires() {
        log_refused(lease, error_code::SCHEDULE_NOT_RUNNABLE);
        return Err(Refusal::conflict(
            error_code::SCHEDULE_NOT_RUNNABLE,
            DETAIL_NOT_RUNNABLE,
            target.desired_status.as_str(),
        ));
    }
    if lease.actor.starts_with(ACTOR_PREFIX) {
        log_refused(lease, error_code::SCHEDULE_NOT_RUNNABLE);
        return Err(Refusal::conflict(
            error_code::SCHEDULE_NOT_RUNNABLE,
            DETAIL_SCHEDULED_RUN,
            STATE_SCHEDULED_RUN,
        ));
    }
    Ok(())
}
