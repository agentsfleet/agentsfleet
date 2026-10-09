//! `GET|POST /v1/runners/me/memory/{fleet_id}` — what a fleet remembers.
//!
//! # The runner NAMES the fleet
//!
//! It already holds the fleet in its lease payload, so naming it explicitly
//! beats inferring it from ambient lease state — and it gives the write path an
//! IDOR cross-check to make: the body's lease must belong to the path's fleet.
//!
//! # Two verbs, one path, different authorization
//!
//! GET asks whether this runner holds a live lease on this fleet. POST asks
//! that AND fences the token, because it writes. Both refusals are the
//! statement's `WHERE`, decided in `afd_fleet` — this layer supplies the
//! identity and renders the answer.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_wire::memory::{MemoryCaptureResponse, MemoryPushRequest, MemoryRecallRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use crate::auth::RunnerIdentity;
use crate::handler::{malformed, refuse};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed hydrate is logged under.
const EVENT_HYDRATE: &str = "runner_memory_hydrate_failed";

/// The scoped event a failed capture is logged under.
const EVENT_CAPTURE: &str = "runner_memory_capture_failed";

/// The refusal a path segment that is not an identifier earns.
const DETAIL_FLEET_ID: &str = "fleet_id must be a valid UUIDv7";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed memory body";

/// The scoped event a failed recall is logged under.
const EVENT_RECALL: &str = "runner_memory_recall_failed";

/// The refusal a recall body this daemon cannot read, or will not search, earns.
const DETAIL_RECALL_MALFORMED: &str = "Malformed memory recall body";

/// Seeds a run with its fleet's memory window.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = afd_wire::paths::RUNNER_MEMORY_FLEET,
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "runner_hydrate_memory",
    summary = "Load what a fleet remembers",
    description = concat!(
        "The memory a fleet carries between runs, read at the start of one. ",
        "The runner names the fleet, and the lease it holds is what makes ",
        "that name legitimate. ",
    ),
    params(
        afd_http::openapi::path::FleetOnly,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_wire::memory::MemoryHydrateResponse),
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
pub(crate) async fn hydrate<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(fleet_id): Path<String>,
) -> Response {
    let Ok(fleet) = Uuid7::parse(&fleet_id) else {
        return malformed(DETAIL_FLEET_ID);
    };
    match services
        .leases()
        .hydrate(runner.id(), &fleet, services.now())
        .await
    {
        Ok(hydrated) => Json(hydrated).into_response(),
        Err(error) => refuse(&error, EVENT_HYDRATE),
    }
}

/// Searches a fleet's memory past the window a run was seeded with.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::RUNNER_MEMORY_RECALL,
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "runner_recall_memory",
    summary = "Search what a fleet remembers",
    description = concat!(
        "Finds the fleet's entries whose key or content holds the query, ",
        "ignoring case, key matches first. A fleet granted to read shared ",
        "memory also gets the workspace's shared entries, each naming the ",
        "fleet that wrote it. Fenced like a capture: a superseded holder is ",
        "refused. ",
    ),
    request_body = MemoryRecallRequest,
    params(
        afd_http::openapi::path::FleetOnly,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = afd_wire::memory::MemoryRecallResponse),
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
pub(crate) async fn recall<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(fleet_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(fleet) = Uuid7::parse(&fleet_id) else {
        return malformed(DETAIL_FLEET_ID);
    };
    let Ok(request) = afd_http::handler::read_strict_body::<MemoryRecallRequest<'_>>(&body) else {
        return malformed(DETAIL_RECALL_MALFORMED);
    };
    if request.validate().is_err() {
        return malformed(DETAIL_RECALL_MALFORMED);
    }
    match services
        .leases()
        .recall(runner.id(), &fleet, &request, services.now())
        .await
    {
        Ok(recalled) => Json(recalled).into_response(),
        Err(error) => refuse(&error, EVENT_RECALL),
    }
}

/// Persists what a run learned.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::RUNNER_MEMORY_FLEET,
    tag = afd_http::openapi::tag::MEMORY,
    operation_id = "runner_capture_memory",
    summary = "Capture what a fleet learned",
    description = concat!(
        "Writes back what the run decided is worth keeping. The reply names ",
        "what was stored and what was skipped, so a runner learns which of ",
        "its entries did not survive the bounds. ",
    ),
    request_body = MemoryPushRequest,
    params(
        afd_http::openapi::path::FleetOnly,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = MemoryCaptureResponse),
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
pub(crate) async fn capture<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(fleet_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(fleet) = Uuid7::parse(&fleet_id) else {
        return malformed(DETAIL_FLEET_ID);
    };
    // Borrowed out of `body`: every delta's content goes straight into a column.
    // Strict because a delta is also what hydrate answers with, and the runner
    // reads that leniently; a pushed delta still carries exactly three keys.
    let Ok(request) = afd_http::handler::read_strict_body::<MemoryPushRequest<'_>>(&body) else {
        return malformed(DETAIL_MALFORMED);
    };

    match services
        .leases()
        .capture(runner.id(), &fleet, &request, services.now())
        .await
    {
        // The tallies only; the sweep and eviction counts stay in the log,
        // being the daemon's housekeeping rather than a fact about this
        // request. `MemoryCaptureResponse` carries the reasoning for the two
        // that survive.
        Ok(counted) => Json(MemoryCaptureResponse {
            stored: counted.stored,
            skipped: counted.skipped,
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT_CAPTURE),
    }
}
