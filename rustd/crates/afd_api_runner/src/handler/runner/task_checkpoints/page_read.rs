//! Read one bounded manifest page through the existing authenticated task owner.

use std::sync::Arc;
use afd_fleet::task::TaskLease;
use afd_http::handler::{Refusal, parse_id};
use afd_wire::task::CheckpointReadQuery;
use axum::{Json, extract::{Path, Query, State, rejection::QueryRejection}, response::{IntoResponse as _, Response}};
use http::{StatusCode, header};
use crate::{auth::RunnerIdentity, services::Services};

#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = afd_wire::paths::LEASE_TASK_CHECKPOINT_PAGE, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_checkpoint_page_read", summary = "Read one bounded manifest page",
    description = "Requires the current live lease and native restore pin. Returns one base or overlay page; the supervisor verifies its digest and the ordered root before attaching files.",
    params(("lease_id" = String, Path, description = "Native lease UUIDv7."),
        ("checkpoint_id" = String, Path, description = "Native checkpoint UUIDv7."),
        ("page_id" = String, Path, description = "Native page UUIDv7."), CheckpointReadQuery),
    responses((status = 200, body = afd_wire::task::CheckpointPage, description = "Native checkpoint result"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN), (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT), (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS), (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn handle<D: Services>(State(services): State<Arc<D>>, RunnerIdentity(runner): RunnerIdentity,
    Path((lease, checkpoint, page)): Path<(String, String, String)>, query: Result<Query<CheckpointReadQuery>, QueryRejection>) -> Result<Response, Refusal> {
    let lease_id = parse_id(&lease, "lease_id must be a UUIDv7")?;
    let checkpoint_id = parse_id(&checkpoint, "checkpoint_id must be a UUIDv7")?;
    let page_id = parse_id(&page, "page_id must be a UUIDv7")?;
    let Query(value) = query.map_err(|_invalid| Refusal::malformed(super::MALFORMED))?;
    let input = garde::Unvalidated::new(value).validate().map_err(|_invalid| Refusal::malformed(super::MALFORMED))?;
    let lease = TaskLease { runner_id: runner.id(), lease_id: &lease_id, fencing_token: input.fencing_token };
    let result = services.tasks().checkpoint_page(lease, &checkpoint_id, &page_id, &input).await.map_err(Refusal::at(super::EVENT))?;
    let status = StatusCode::OK;
    Ok((status, [(header::CACHE_CONTROL, "no-store")], Json(result)).into_response())
}
