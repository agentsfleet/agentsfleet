//! `POST /v1/runners/me/leases/{lease_id}/renew` — a live run buying more time.
//!
//! # The body is optional and its failure is not
//!
//! `RenewRequest` carries one line per provider request since the last
//! recorded renewal, and a renewal that saw none may send no body at all. A
//! body that will not read, or breaks a bound, still extends the lease and
//! bills the run time the daemon measured, rather than refusing: refusing
//! kills a healthy child over a serialization disagreement. Its lines are
//! simply not recorded, and the answer's `recorded_through` says so, so the
//! runner sends them again. `UZ-RUN-013` exists in the registry for this and
//! is deliberately never ANSWERED, only logged.
//!
//! # Why the lease id comes off the path
//!
//! One lease per request, named where a reader of the access log can see it.
//! The ownership check is not here — the load is scoped by runner id inside the
//! plane, so a runner naming a peer's lease gets the same 404 a missing one
//! gets, and this layer never has to know that is what happened.

use std::sync::Arc;

use afd_wire::report::{RenewRequest, RenewResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed renewal is logged under.
const EVENT: &str = "runner_renew_failed";

/// A body arrived and could not be read; the slice meters runtime only.
const EVENT_BODY_INVALID: &str = "renew_body_parse_failed";

/// Extends one live lease's deadline.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::LEASE_RENEW,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_renew_lease",
    summary = "Extend a live lease",
    description = concat!(
        "A run buying more time before its lease expires. The body is ",
        "optional and carries one line per provider request since the last ",
        "recorded renewal. The answer's `recorded_through` names the highest ",
        "line recorded, and the runner sends the rest again. A body that ",
        "will not parse records no lines and is logged as ",
        "`renew_body_parse_failed`. The lease is still extended, so a ",
        "counting mistake never ends a run. ",
    ),
    request_body = Option<RenewRequest>,
    params(
        afd_http::openapi::path::Lease,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RenewResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
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
) -> Response {
    // Parsed first, so the lease lookup's `::uuid` cast never decides the
    // answer: a value that cannot name any lease is the caller's mistake.
    if let Err(refusal) = super::schedule::lease(&lease_id) {
        return refusal.into_response();
    }
    let request = requests(&body);
    let recorded_through = request
        .requests
        .0
        .iter()
        .map(|line| line.seq)
        .max();
    match services
        .leases()
        .renew(runner.id(), &lease_id, request, services.now())
        .await
    {
        Ok(expires_at) => Json(RenewResponse {
            lease_expires_at: expires_at,
            recorded_through,
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The request lines the body carried, or none.
///
/// Default-safe by design; see this module's note. The empty case is silent
/// because a renewal that saw no provider request sends nothing; a body that
/// arrived and would not read, or broke a bound, is logged, because that one
/// means two builds disagree about a shape and somebody should know.
fn requests(body: &[u8]) -> RenewRequest {
    if body.is_empty() {
        return RenewRequest::default();
    }
    afd_http::handler::read_body::<RenewRequest>(body)
        .ok()
        .filter(|request| garde::Validate::validate(request).is_ok())
        .unwrap_or_else(|| {
            tracing::warn!(
                event = EVENT_BODY_INVALID,
                "the renew body could not be read; its lines are not recorded and the runner sends them again"
            );
            RenewRequest::default()
        })
}
