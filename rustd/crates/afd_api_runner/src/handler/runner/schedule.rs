//! `/v1/runners/me/leases/{lease_id}/schedules` — a running fleet keeping its
//! own follow-ups on the daemon's schedule plane.
//!
//! # The fleet is the lease's
//!
//! Every verb here proves the lease first (`afd_fleet::lease::Standing`) and
//! acts on the fleet that lease runs. Nothing in a body or a query names a
//! fleet, so a runner cannot reach another fleet's schedules, and a holder a
//! reclaim has superseded is refused before any schedule is read.
//!
//! # A fleet reads every schedule and changes only its own
//!
//! The list shows a person's schedules beside the fleet's, each naming its
//! source, so the model plans around what exists. An edit or a delete of a
//! schedule a person made answers `UZ-SCHED-010`. The store, the reconcile to
//! QStash and every rendering are the tenant surface's own, so a schedule a
//! fleet made behaves exactly as one a person made.
//!
//! [`create`] and [`list`] are here, with the helpers the sibling verbs share;
//! the edit and the delete are in `schedule_edit`, the runs in `schedule_run`.

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code::ErrorCode;
use afd_core::id::Uuid7;
use afd_cron::model::DEFAULT_TIMEZONE;
use afd_cron::{NewSchedule, Reconciled, Source, validate};
use afd_fleet::lease::Standing;
use afd_http::handler::schedule::{checked, not_fleet_owned, not_found, refused, rendered};
use afd_http::handler::{Refusal, parameter, parse_id, read_strict_body};
use afd_wire::schedule::Page;
use afd_wire::schedule_verb::{QUERY_FENCING_TOKEN, ScheduleCreateRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::RunnerIdentity;
use crate::services::{FleetSchedules as _, Leasing as _, Services};

/// The scoped event a schedules verb that failed is logged under.
pub(super) const EVENT_FAILED: &str = "runner_schedules_failed";

/// A fleet created a schedule.
const EVENT_CREATED: &str = "fleet_schedule_created";

/// A fleet's schedule write was refused for a bound, an owner or a field.
const EVENT_REFUSED: &str = "fleet_schedule_refused";

/// The refusal a body this verb cannot read earns.
pub(super) const DETAIL_MALFORMED: &str = "Malformed schedules body";

/// The refusal a lease path segment that is not an identifier earns.
const DETAIL_LEASE_ID: &str = "lease_id must be a valid UUIDv7";

/// The refusal a schedule path segment that is not an identifier earns.
const DETAIL_SCHEDULE_ID: &str = "schedule_id must be a valid UUIDv7";

/// The refusal a read or a delete without its fence earns.
pub(super) const DETAIL_FENCE: &str = "fencing_token must be the lease's fencing token";

/// The view a create and an edit answer with.
#[cfg(feature = "openapi")]
pub(super) const RECONCILED: &str = "The schedule as reconciled with the scheduler";

/// Creates a schedule for the fleet the lease runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_create_schedule",
    summary = "Create a schedule for the running fleet",
    description = concat!(
        "Creates a schedule for the fleet the lease runs, with source `fleet`, ",
        "and registers it in QStash as a person's schedule is. The fleet comes ",
        "from the lease and never from the body. A fleet holds at most 16 ",
        "schedules it created, refused past that with `UZ-SCHED-009`. With ",
        "`once` set, the schedule retires after its first fire. A schedule ",
        "that saved and did not register answers 201 with its `sync` state. ",
    ),
    request_body = ScheduleCreateRequest,
    params(afd_http::openapi::path::Lease),
    responses(
        (status = 201, description = RECONCILED, body = afd_wire::schedule::View),
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
pub(crate) async fn create<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: ScheduleCreateRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    let now = services.now();
    let lease = standing(
        &*services,
        runner.id(),
        &lease_id,
        request.fencing_token,
        now,
    )
    .await?;
    let timezone = request.timezone.as_deref().unwrap_or(DEFAULT_TIMEZONE);
    checked(validate::Fields {
        expression: Some(&request.cron),
        timezone: Some(timezone),
        message: Some(&request.message),
    })
    .inspect_err(|_invalid| log_refused(&lease, afd_core::error_code::INVALID_REQUEST))?;
    // Prefixed by the event that created it, so the key says which run made
    // the schedule until QStash's own id replaces it; the instant keeps two
    // schedules one run makes apart, because the key is unique per fleet.
    let source_key = format!("{}-{}", lease.event_id, now.as_millis());
    let created = services
        .schedules()
        .create(
            &lease.workspace_id,
            NewSchedule {
                fleet: &lease.fleet_id,
                source: Source::Fleet,
                source_key: &source_key,
                cron: &request.cron,
                timezone,
                message: &request.message,
                once: request.once,
            },
            now,
        )
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let reconciled = created.map_err(|refusal| {
        log_refused(&lease, refusal.code());
        refused(refusal)
    })?;
    if let Reconciled::Synced(schedule) | Reconciled::Failed(schedule) = &reconciled {
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let fleet_id = lease.fleet_id.as_str();
        let schedule_id = schedule.schedule_id.as_str();
        let agentsfleet_event_id = lease.event_id.as_str();
        tracing::info!(
            fleet_id,
            schedule_id,
            agentsfleet_event_id,
            event = EVENT_CREATED
        );
    }
    rendered(reconciled, StatusCode::CREATED)
}

/// Lists every schedule of the fleet the lease runs.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/runners/me/leases/{lease_id}/schedules",
    tag = afd_http::openapi::tag::SCHEDULES,
    operation_id = "runner_list_schedules",
    summary = "List the running fleet's schedules",
    description = concat!(
        "Lists every schedule of the fleet the lease runs, oldest first, each ",
        "naming its `source`: `api` for a person, `trigger` for the fleet's ",
        "own document, `fleet` for one the fleet made. Only a `fleet` schedule ",
        "can be changed through the lease. ",
    ),
    params(afd_http::openapi::path::Lease, afd_http::openapi::query::Fence),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = Page),
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
pub(crate) async fn list<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    RawQuery(query): RawQuery,
) -> Result<Response, Refusal> {
    let now = services.now();
    let token = fence(query.as_deref())?;
    let lease = standing(&*services, runner.id(), &lease_id, token, now).await?;
    let schedules = services
        .schedules()
        .list(&lease.fleet_id)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Json(Page {
        schedules: schedules.iter().map(afd_cron::Schedule::view).collect(),
    })
    .into_response())
}

/// Proves `lease_id` is `runner`'s live lease under `token`.
pub(super) async fn standing<D: Services>(
    services: &D,
    runner: &Uuid7,
    lease_id: &str,
    token: u64,
    now: UnixMillis,
) -> Result<Standing, Refusal> {
    let lease = parse_id(lease_id, DETAIL_LEASE_ID)?;
    services
        .leases()
        .standing(runner, lease, token, now)
        .await
        .map_err(Refusal::at(EVENT_FAILED))
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

/// Refuses unless `schedule` is the lease's fleet's, and one the fleet made.
pub(super) async fn fleet_owned<D: Services>(
    services: &D,
    lease: &Standing,
    schedule: &Uuid7,
) -> Result<(), Refusal> {
    let found = services
        .schedules()
        .one(&lease.fleet_id, schedule)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?
        .ok_or_else(not_found)?;
    if found.source == Source::Fleet {
        Ok(())
    } else {
        log_refused(lease, afd_core::error_code::SCHEDULE_NOT_FLEET_OWNED);
        Err(not_fleet_owned())
    }
}

/// Logs a refused schedule write by its code; never the body.
pub(super) fn log_refused(lease: &Standing, code: ErrorCode) {
    // Hoisted: see the `tracing` note in the workspace Cargo.toml.
    let fleet_id = lease.fleet_id.as_str();
    let error_code = code.as_str();
    tracing::info!(fleet_id, error_code, event = EVENT_REFUSED);
}
