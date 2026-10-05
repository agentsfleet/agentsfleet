//! `/v1/runners/me/leases/{lease_id}/schedules/{schedule_id}/runs` — one
//! schedule's runs: the ones that ran, and a new one to fire it now.
//!
//! A run is an event whose actor is `cron:<schedule_id>`, which a QStash fire
//! and a run-now both record (`afd_cron::schedule_actor`). Running now admits
//! through the same producer QStash's callback does, keyed by the lease, so a
//! retried post answers the run it already created rather than a second one.
//! A run joins this list once a runner has taken it: until then it is queued
//! work, not history.

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::paging::{CEILING, QUERY_LIMIT, QUERY_STARTING_AFTER};
use afd_cron::schedule_actor;
use afd_events::{Cursor, EventRow, Filter, next_cursor, prefix_to_like};
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

use super::schedule::{DETAIL_MALFORMED, EVENT_FAILED, fence, schedule_id, standing};
use crate::auth::RunnerIdentity;
use crate::services::{FleetSchedules as _, Services, WorkspaceEvents as _};

/// The refusal a page size outside the served band earns.
const DETAIL_LIMIT: &str = "limit must be between 1 and 100";

/// The refusal a cursor this daemon did not mint earns.
const DETAIL_CURSOR: &str = "starting_after must be a next_cursor this daemon returned";

/// What a run-now's admission key names after the schedule: the lease that
/// asked, so one lease's retries of one run are one run.
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
        "had fired it: one event with actor `cron:<schedule_id>`, queued for ",
        "the fleet. Posting again under the same lease answers the same run. A ",
        "`once` schedule retires after this run. ",
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
    let key = format!("{RUN_KEY_PREFIX}{}", lease.lease_id);
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
        "Page with `starting_after` and `limit`. ",
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
    services
        .schedules()
        .one(&lease.fleet_id, &schedule)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?
        .ok_or_else(not_found)?;
    let filter = Filter {
        actor_like: Some(prefix_to_like(&schedule_actor(&schedule))),
        since: None,
    };
    let rows = i64::from(limit);
    let page = services
        .events()
        .page_for_fleet(
            &lease.workspace_id,
            &lease.fleet_id,
            &filter,
            cursor.as_ref(),
            rows,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Json(EventsResponse {
        items: page.iter().map(EventRow::summary).collect(),
        next_cursor: next_cursor(&page, rows).map(|after| Cow::Owned(after.encode())),
    })
    .into_response())
}
