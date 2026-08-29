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
use afd_wire::memory::{MemoryCaptureResponse, MemoryHydrateResponse, MemoryPushRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

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

/// Seeds a run with its fleet's memory window.
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
        Ok(memory) => Json(MemoryHydrateResponse { memory }).into_response(),
        Err(error) => refuse(&error, EVENT_HYDRATE),
    }
}

/// Persists what a run learned.
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
    let Ok(request) = afd_core::json::object_from_slice::<MemoryPushRequest<'_>>(&body) else {
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
