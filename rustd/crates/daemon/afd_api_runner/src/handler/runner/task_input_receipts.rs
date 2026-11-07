//! Acknowledge current input application or the original source's reply obligation.

use crate::{auth::RunnerIdentity, services::Services};
use af_wire::task::TaskInputReceiptInput;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

const EVENT: &str = "runner_task_input_receipt_failed";
const MALFORMED: &str = "Input receipt exceeds its supported shape or bounds";

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = af_wire::paths::LEASE_TASK_INPUT_RECEIPTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_accept_task_input_receipt", summary = "Acknowledge accepted input and its original reply",
    description = "Fences apply-here and queued acknowledgements against immutable accepted meaning. Advances the current input cursor once, preserves original delivery obligations and never runs a different task under the current lease.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7.")), request_body = TaskInputReceiptInput,
    responses((status = 201, body = af_wire::task::TaskInputReceiptResult, description = "Accepted input receipt"),
        (status = 200, body = af_wire::task::TaskInputReceiptResult, description = "Immutable receipt replay"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let input: TaskInputReceiptInput =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let result = services
        .tasks()
        .accept_input(lease, &input)
        .await
        .map_err(Refusal::at(EVENT))?;
    let status = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
