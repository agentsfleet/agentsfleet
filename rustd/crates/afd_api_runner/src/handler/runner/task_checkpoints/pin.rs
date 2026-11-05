//! Renew a current capture or restore pin through the existing authenticated task owner.

use std::sync::Arc;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id};
use afd_wire::task::CheckpointPinInput;
use axum::{Json, body::Bytes, extract::{Path, State}, response::{IntoResponse as _, Response}};
use http::{StatusCode, header};
use crate::{auth::RunnerIdentity, services::Services};

#[cfg_attr(feature = "openapi", utoipa::path(
    patch, path = afd_wire::paths::LEASE_TASK_CHECKPOINT_PIN, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_checkpoint_pin", summary = "Renew a current capture or restore pin",
    description = "Requires matching task, fence and pin versions. Renewal cannot extend the original transfer deadline or revive expired, stopped or superseded authority.",
    params(("lease_id" = String, Path, description = "Native lease UUIDv7."),
        ("checkpoint_id" = String, Path, description = "Native checkpoint UUIDv7."),
        ("pin_id" = String, Path, description = "Native pin UUIDv7.")), request_body = CheckpointPinInput,
    responses((status = 200, body = afd_wire::task::CheckpointPin, description = "Native checkpoint result"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn handle<D: Services>(State(services): State<Arc<D>>, RunnerIdentity(runner): RunnerIdentity,
    Path((lease, checkpoint, pin)): Path<(String, String, String)>, body: Bytes) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let checkpoint_id = parse_id(&checkpoint, "checkpoint_id must be a UUIDv7")?;
    let pin_id = parse_id(&pin, "pin_id must be a UUIDv7")?;
    let input: garde::Valid<CheckpointPinInput> = super::input(&body)?;
    let lease = TaskLease { runner_id: runner.id(), lease_id: &lease_id, fencing_token: input.fencing_token };
    let result = services.tasks().renew_checkpoint_pin(lease, &checkpoint_id, &pin_id, &input).await.map_err(Refusal::at(super::EVENT))?;
    let status = StatusCode::OK;
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
