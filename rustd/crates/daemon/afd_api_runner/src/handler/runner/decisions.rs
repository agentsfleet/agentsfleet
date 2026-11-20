//! Thin run-scoped adapters; native task code owns funding and accounting.

use std::sync::Arc;

use af_wire::task::{DecisionReceiptInput, DecisionReservationInput};
use afd_fleet::task::TaskRun;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};

use crate::{auth::RunnerIdentity, services::RunnerPlane};

const EVENT: &str = "runner_task_decision_failed";
const MALFORMED: &str = "Decision request exceeds its supported shape or bounds";
const RUN_ID: &str = "run_id must be a UUIDv7";
const CALL_ID: &str = "call_id must be a UUIDv7";

/// Native issuance commits once; an operation retry never receives another key.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = af_wire::paths::RUN_TASK_DECISIONS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_reserve_task_decision", summary = "Reserve a physical task decision",
    description = "Uses the run's reviewed decision profile, native source and accepted questions. Confirmed credit and fresh platform Secrets are required before one credential response. Identical retries return saved state without another credential.",
    params(("run_id" = String, Path, description = "Original run UUIDv7.")),
    request_body = DecisionReservationInput,
    responses(
        (status = 201, description = "One physical send authorized", body = af_wire::task::DecisionReservationResult<'_>),
        (status = 200, description = "Saved call without another credential", body = af_wire::task::DecisionReservationResult<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn reserve<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let run_id = parse_id(&run, RUN_ID)?;
    let input: DecisionReservationInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let run = TaskRun {
        runner_id: runner.id(),
        run_id: &run_id,
        fencing_token: input.fencing_token,
    };
    let response = services
        .tasks()
        .reserve_decision(run, &input)
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

/// Accounting survives deleted task/run state and never grants transition authority.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = af_wire::paths::RUN_TASK_DECISION, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_record_task_decision", summary = "Record a physical task decision receipt",
    description = "Accounts usage under the original runner, run and fence, even after live work is removed. Known charges and unknown reservations are independent of answer usability. Canonical retries do not charge twice.",
    params(("run_id" = String, Path, description = "Original run UUIDv7."),
        ("call_id" = String, Path, description = "Native physical decision call UUIDv7.")),
    request_body = DecisionReceiptInput,
    responses(
        (status = 200, description = "Current accounting and answer disposition", body = af_wire::task::DecisionReceiptResult),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    )
))]
pub(crate) async fn record<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((run, call)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let run_id = parse_id(&run, RUN_ID)?;
    let call_id = parse_id(&call, CALL_ID)?;
    let input: DecisionReceiptInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let run = TaskRun {
        runner_id: runner.id(),
        run_id: &run_id,
        fencing_token: input.fencing_token,
    };
    let recorded = services
        .tasks()
        .record_decision(run, &call_id, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(recorded)).into_response())
}
