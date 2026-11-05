//! Pending inputs are complete native messages; this route never consumes them.

use std::sync::Arc;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id};
use afd_wire::task::TaskInputsQuery;
use axum::{Json, extract::{Path, Query, State, rejection::QueryRejection}, response::{IntoResponse as _, Response}};

use crate::{auth::RunnerIdentity, services::Services};

const MALFORMED: &str = "Task input query exceeds its supported shape or bounds";
const EVENT: &str = "runner_task_inputs_failed";

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = afd_wire::paths::LEASE_TASK_INPUTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_read_task_inputs", summary = "Read pending human task input",
    description = "Returns up to four complete human messages from the current source conversation in native FIFO order, with current task summaries. Reads never claim or consume input. Requires the current runner, live lease and fence.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7."), TaskInputsQuery),
    responses((status = 200, body = afd_wire::task::TaskInputsResponse, description = "Complete pending input and native work"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED), (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND), (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn read<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    query: Result<Query<TaskInputsQuery>, QueryRejection>,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let Query(query) = query.map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(query).validate().map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease { runner_id: runner.id(), lease_id: &lease_id, fencing_token: input.fencing_token };
    let page = services.tasks().inputs(lease, &input).await.map_err(Refusal::conflict_or_at(EVENT, conflict))?;
    Ok(([(http::header::CACHE_CONTROL, "no-store")], Json(page)).into_response())
}

fn conflict(error: &afd_fleet::Error) -> Option<&'static str> {
    matches!(error.code(), afd_core::error_code::RUN_STALE_FENCING_TOKEN | afd_core::error_code::TASK_STALE_STATE)
        .then_some("superseded")
}
