//! The two verbs that answer from the store and change nothing.
//!
//! Neither reaches the external scheduler. A row this daemon holds is the whole
//! answer, including its `sync` field — a caller asking what is registered
//! reads what the last reconcile recorded, not a fresh call to `QStash`, because
//! a read that could fail upstream is a read that fails when the scheduler is
//! down and the schedule is fine.

use std::sync::Arc;

use afd_core::error_code;
use afd_wire::schedule::Page;
/// Named only by the `body =` clause of this module's `utoipa::path`
/// annotations, which the default build compiles away — so the import has to
/// go with them or the feature-off build fails on an unused name.
#[cfg(feature = "openapi")]
use afd_wire::schedule::View;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::handler::Refusal;
use crate::handler::fleet::detail::parse_fleet_id;
use crate::services::{FleetSchedules as _, Services};

use super::support::view_of;
use super::{DETAIL_NOT_FOUND, EVENT_READ};

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
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
