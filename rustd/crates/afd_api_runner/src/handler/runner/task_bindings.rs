//! Complete native message acceptance; this adapter cannot prepare a target itself.

use std::sync::Arc;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_wire::task::TaskBindingsInput;
use axum::{Json, body::Bytes, extract::{Path, State}, response::{IntoResponse as _, Response}};
use http::{StatusCode, header};
use crate::{auth::RunnerIdentity, services::Services};

const EVENT: &str = "runner_task_binding_failed";
const MALFORMED: &str = "Task interpretation exceeds its supported shape or bounds";

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::LEASE_TASK_BINDINGS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_accept_task_bindings", summary = "Accept a complete native task interpretation",
    description = "Checks accounted current intent and association answers, whole-message multiplicity, pinned item digests and native source spans. Accepts every item in one transaction. Conversation never creates a task; independent new work joins the existing FIFO admission path. Existing PR targets require exact human assignment, reviewed workflows and current grants.",
    params(("lease_id" = String, Path, description = "Current lease UUIDv7.")), request_body = TaskBindingsInput,
    responses((status = 201, body = afd_wire::task::TaskBindingsResponse, description = "Complete interpretation accepted"),
        (status = 200, body = afd_wire::task::TaskBindingsResponse, description = "Immutable interpretation replay"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn accept<D: Services>(State(services): State<Arc<D>>, RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>, body: Bytes) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let input: TaskBindingsInput = read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let input = garde::Unvalidated::new(input).validate().map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let lease = TaskLease { runner_id: runner.id(), lease_id: &lease_id, fencing_token: input.fencing_token };
    let response = services.tasks().interpret(lease, &input).await.map_err(Refusal::at(EVENT))?;
    let status = if response.replayed { StatusCode::OK } else { StatusCode::CREATED };
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(response)).into_response())
}
