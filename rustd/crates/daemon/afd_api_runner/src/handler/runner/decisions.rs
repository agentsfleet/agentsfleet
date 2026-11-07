//! Thin lease-scoped adapters; native task code owns funding and accounting.

use std::sync::Arc;

use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_wire::task::{DecisionReceiptInput, DecisionReservationInput};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};

use crate::{auth::RunnerIdentity, services::Services};

const EVENT: &str = "runner_task_decision_failed";
const MALFORMED: &str = "Decision request exceeds its supported shape or bounds";
const LEASE_ID: &str = "lease_id must be a UUIDv7";
const CALL_ID: &str = "call_id must be a UUIDv7";

/// Native issuance commits once; an operation retry never receives another key.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::LEASE_TASK_DECISIONS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_reserve_task_decision", summary = "Reserve a physical task decision",
    description = "Uses the lease's reviewed decision profile, native source and accepted questions. Confirmed credit and fresh platform Secrets are required before one credential response. Identical retries return saved state without another credential.",
    params(("lease_id" = String, Path, description = "Original lease UUIDv7.")),
    request_body = DecisionReservationInput,
    responses(
        (status = 201, description = "One physical send authorized", body = afd_wire::task::DecisionReservationResult),
        (status = 200, description = "Saved call without another credential", body = afd_wire::task::DecisionReservationResult),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE)
    )
))]
pub(crate) async fn reserve<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, LEASE_ID)?;
    let input: DecisionReservationInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let response = services
        .tasks()
        .reserve_decision(lease, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    let (issued, body) = response.into_parts();
    let status = if issued {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response())
}

/// Accounting survives deleted task/lease state and never grants transition authority.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = afd_wire::paths::LEASE_TASK_DECISION, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_record_task_decision", summary = "Record a physical task decision receipt",
    description = "Accounts usage under the original runner, lease and fence, even after live work is removed. Known charges and unknown reservations are independent of answer usability. Canonical retries do not charge twice.",
    params(("lease_id" = String, Path, description = "Original lease UUIDv7."),
        ("call_id" = String, Path, description = "Native physical decision call UUIDv7.")),
    request_body = DecisionReceiptInput,
    responses(
        (status = 200, description = "Current accounting and answer disposition", body = afd_wire::task::DecisionReceiptResult),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE)
    )
))]
pub(crate) async fn record<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease, call)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, LEASE_ID)?;
    let call_id = parse_id(&call, CALL_ID)?;
    let input: DecisionReceiptInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let recorded = services
        .tasks()
        .record_decision(lease, &call_id, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(recorded)).into_response())
}
