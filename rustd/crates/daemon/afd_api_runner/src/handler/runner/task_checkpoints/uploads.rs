//! Freeze exact ciphertext upload descriptors through the existing authenticated task owner.

use crate::{auth::RunnerIdentity, services::Services};
use af_wire::task::CheckpointUploadsInput;
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
    post, path = af_wire::paths::LEASE_TASK_CHECKPOINT_UPLOADS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_checkpoint_uploads", summary = "Freeze exact ciphertext upload descriptors",
    description = "Records every ciphertext digest and length before signing create-only uploads. Exact replay cannot choose a new key, nonce or representation.",
    params(("lease_id" = String, Path, description = "Native lease UUIDv7."),
        ("checkpoint_id" = String, Path, description = "Native checkpoint UUIDv7.")), request_body = CheckpointUploadsInput,
    responses((status = 201, body = af_wire::task::CheckpointObjectsPage, description = "Native checkpoint result"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path((lease, checkpoint)): Path<(String, String)>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let checkpoint_id = parse_id(&checkpoint, "checkpoint_id must be a UUIDv7")?;
    let input: garde::Valid<CheckpointUploadsInput> = super::input(&body)?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let result = services
        .tasks()
        .checkpoint_uploads(lease, &checkpoint_id, &input)
        .await
        .map_err(Refusal::at(super::EVENT))?;
    let status = StatusCode::CREATED;
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
