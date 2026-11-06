//! Allocate a fenced checkpoint capture plan through the existing authenticated task owner.

use crate::{auth::RunnerIdentity, services::Services};
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id};
use afd_wire::task::CheckpointPlanInput;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::LEASE_TASK_CHECKPOINTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_checkpoint_plan", summary = "Allocate a fenced checkpoint capture plan",
    description = "Binds the native repository, cumulative byte ceilings, immutable ordered page identities and one expiring writer pin before any upload. Replays preserve the original operation and deadlines.",
    params(("lease_id" = String, Path, description = "Native lease UUIDv7.")), request_body = CheckpointPlanInput,
    responses((status = 201, body = afd_wire::task::CheckpointPlan, description = "Native checkpoint result"),
        (status = 200, body = afd_wire::task::CheckpointPlan, description = "Immutable checkpoint plan replay"),
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
    let input: garde::Valid<CheckpointPlanInput> = super::input(&body)?;
    let lease = TaskLease {
        runner_id: runner.id(),
        lease_id: &lease_id,
        fencing_token: input.fencing_token,
    };
    let result = services
        .tasks()
        .plan_checkpoint(lease, &input)
        .await
        .map_err(Refusal::at(super::EVENT))?;
    let status = if result.replayed {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
