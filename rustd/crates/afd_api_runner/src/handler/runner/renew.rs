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
//! runner sends them again. That case is LOGGED under `UZ-RUN-013`.
//!
//! # The one body that is refused
//!
//! The cumulative totals runners sent before usage moved to request lines
//! (`input_tokens`, `cached_input_tokens`, `output_tokens`) are answered 400
//! `UZ-RUN-013`, the field named, and the lease does not renew. That is not a
//! disagreement a later renewal can settle: such a runner never sends a line,
//! so extending its lease would run it unbilled, and its report is refused for
//! the same field anyway. Ending the run at its first renewal is the cheaper
//! of the two endings, and the one that tells its operator why.
//!
//! # Why the lease id comes off the path
//!
//! One lease per request, named where a reader of the access log can see it.
//! The ownership check is not here — the load is scoped by runner id inside the
//! plane, so a runner naming a peer's lease gets the same 404 a missing one
//! gets, and this layer never has to know that is what happened.

use std::sync::Arc;

use afd_core::error_code;
use afd_wire::report::{RenewRequest, RenewResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use super::usage;
use crate::auth::RunnerIdentity;
use crate::handler::{refuse, reject};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed renewal is logged under.
const EVENT: &str = "runner_renew_failed";

/// A body arrived and could not be read; the slice meters runtime only.
const EVENT_BODY_INVALID: &str = "renew_body_parse_failed";

/// A body carried the retired cumulative totals and the renewal was refused.
const EVENT_BODY_RETIRED: &str = "renew_body_retired";

/// What [`handle`] documents, its cap spelled from its constant.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = const_format::concatcp!(
    "Extends the current work assignment. The optional body includes up to ",
    afd_wire::usage::LINES_MAX,
    " provider usage records. `recorded_through` confirms the highest saved record; ",
    "resend later records. Invalid bodies save no records but still renew, logging ",
    "`renew_body_parse_failed`. Retired totals (`input_tokens`, ",
    "`cached_input_tokens`, `output_tokens`) instead return 400 `UZ-RUN-013` without ",
    "renewal; `detail` names the field.",
);

/// Extends one live lease's deadline.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::LEASE_RENEW,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_renew_lease",
    summary = "Extend a live lease",
    description = DESCRIPTION,
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
    let request = match requests(&body) {
        Ok(request) => request,
        Err(detail) => {
            let error_code = error_code::RUN_RENEW_BODY_MALFORMED.as_str();
            let event = EVENT_BODY_RETIRED;
            tracing::warn!(
                event,
                error_code,
                "the renew body carried retired cumulative totals; the lease is not renewed"
            );
            return reject(error_code::RUN_RENEW_BODY_MALFORMED, detail);
        }
    };
    let recorded_through = request.requests.0.iter().map(|line| line.seq).max();
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

/// The request lines the body carried, or none; or the sentence refusing it.
///
/// Default-safe by design; see this module's note. The empty case is silent
/// because a renewal that saw no provider request sends nothing; a body that
/// arrived and would not read, or broke a bound, is logged, because that one
/// means two builds disagree about a shape and somebody should know. Only the
/// retired cumulative shape is refused, with the sentence naming its field.
fn requests(body: &[u8]) -> Result<RenewRequest, &'static str> {
    if body.is_empty() {
        return Ok(RenewRequest::default());
    }
    match afd_http::handler::read_body::<RenewRequest>(body) {
        Ok(request) if garde::Validate::validate(&request).is_ok() => Ok(request),
        Ok(_out_of_bounds) => Ok(unrecorded()),
        Err(failure) => usage::retired_total(&failure).map_or_else(|| Ok(unrecorded()), Err),
    }
}

/// No lines, logged: the renewal still extends and the runner sends them again.
fn unrecorded() -> RenewRequest {
    let error_code = error_code::RUN_RENEW_BODY_MALFORMED.as_str();
    tracing::warn!(
        event = EVENT_BODY_INVALID,
        error_code,
        "the renew body could not be read; its lines are not recorded and the runner sends them again"
    );
    RenewRequest::default()
}
