//! `POST /v1/runners/me/leases/{lease_id}/folds` — the messages a fleet
//! received while its run works, claimed for that run before a model call.
//!
//! The lease plane claims them in one statement that proves the lease, its
//! runner and its fence itself (`afd_fleet::lease::fold`); a lease that is not
//! this runner's, not live, or superseded claims nothing, and the run learns
//! it lost the lease from its renewal. A claim repeated with the same cursor
//! answers the same messages, so the runner may send it again.

use std::sync::Arc;

use af_validate::Sentences;
use af_wire::fold::FoldRequest;
use afd_http::handler::{Refusal, read_strict_body};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use super::schedule::lease;
use crate::auth::RunnerIdentity;
use crate::services::{Leasing as _, Services};

/// The scoped event a claim that failed is logged under.
const EVENT_FAILED: &str = "runner_folds_failed";
/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed folds body";
/// The refusal a cursor outside its bound earns.
const DETAIL_CURSOR: &str = "after_sequence must be 0 or more";
/// The request's refusals, by the path garde names when a bound breaks.
const BOUNDS: Sentences = Sentences::new(&[("after_sequence", DETAIL_CURSOR)], DETAIL_MALFORMED);

/// Claims the messages the fleet received after the run's cursor.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::LEASE_FOLDS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_claim_folds",
    summary = "Claim the messages the fleet received while the run works",
    description = "Claims, for the held lease's run, the messages its fleet received after `after_sequence`, oldest first. It stops at the first parked or task-bound message, and at 32. Each message read by a completed model call is answered by the run's report. A lease that is not live answers no message. The same cursor answers the same messages, so a claim may be sent again.",
    request_body = FoldRequest,
    params(afd_http::openapi::path::Lease),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = af_wire::fold::FoldResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: FoldRequest =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    request
        .validate()
        .map_err(|report| Refusal::malformed(BOUNDS.pick(&report)))?;
    let claimed = services
        .leases()
        .folds(runner.id(), &lease(&lease_id)?, &request)
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    Ok(Json(claimed).into_response())
}
