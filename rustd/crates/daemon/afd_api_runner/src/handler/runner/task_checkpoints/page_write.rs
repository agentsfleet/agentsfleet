//! Freeze one complete manifest page through the existing authenticated task owner.

use crate::{auth::RunnerIdentity, services::Services};
use af_wire::task::CheckpointPageInput;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = af_wire::paths::LEASE_TASK_CHECKPOINT_PAGE, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_checkpoint_page_write", summary = "Freeze one complete manifest page",
    description = "Checks canonical page bytes, native page position, paths and registered object scope. Identical replay succeeds; changed bytes or page position conflict.",
    params(("lease_id" = String, Path, description = "Native lease UUIDv7."),
        ("checkpoint_id" = String, Path, description = "Native checkpoint UUIDv7."),
        ("page_id" = String, Path, description = "Native page UUIDv7.")), request_body = CheckpointPageInput,
    responses((status = 200, body = af_wire::task::CheckpointPage, description = "Native checkpoint result"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease, checkpoint, page)): Path<(String, String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let checkpoint_id = parse_id(&checkpoint, "checkpoint_id must be a UUIDv7")?;
    let page_id = parse_id(&page, "page_id must be a UUIDv7")?;
    let input: garde::Valid<CheckpointPageInput> = super::input(&body)?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let result = services
        .tasks()
        .write_checkpoint_page(lease, &checkpoint_id, &page_id, &input)
        .await
        .map_err(Refusal::at(super::EVENT))?;
    let status = StatusCode::OK;
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
