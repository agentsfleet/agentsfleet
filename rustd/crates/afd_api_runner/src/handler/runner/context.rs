//! Private model-window notes belong to one branch under a live lease.
use super::schedule::lease;
use crate::{
    auth::RunnerIdentity,
    services::{Leasing as _, Services},
};
use afd_http::handler::{Refusal, read_strict_body};
use afd_wire::context::{ContextLoaded, ContextPath, ContextPut, ContextQuery};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, Query, State},
    response::{IntoResponse as _, Response},
};
use std::sync::Arc;

const EVENT: &str = "runner_context_checkpoint_failed";
const MALFORMED: &str = "Malformed context checkpoint";
const SUPERSEDED: &str = "superseded";

/// Load a private checkpoint under the current assignment token.
#[cfg_attr(feature = "openapi", utoipa::path(
    get, path = afd_wire::paths::LEASE_CONTEXT, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_load_context_checkpoint", summary = "Load private run working notes",
    description = "Requires the current assignment. Notes belong only to this run branch and are separate from fleet memory.",
    params(ContextPath, ContextQuery),
    responses((status = 200, description = afd_http::openapi::OK, body = ContextLoaded),
        (status = 400, description = afd_http::openapi::BAD_REQUEST), (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND), (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL), (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn load<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(ContextPath {
        lease_id,
        branch_id,
    }): Path<ContextPath>,
    Query(query): Query<ContextQuery>,
) -> Result<Response, Refusal> {
    let query = garde::Unvalidated::new(query)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let checkpoint = services
        .leases()
        .context_load(
            runner.id(),
            &lease(&lease_id)?,
            &lease(&branch_id)?,
            query.fencing_token,
        )
        .await
        .map_err(Refusal::conflict_or_at(EVENT, conflict))?;
    let checkpoint = checkpoint
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    Ok(Json(ContextLoaded { checkpoint }).into_response())
}

/// Replace private notes conditionally; identical retries have one durable effect.
#[cfg_attr(feature = "openapi", utoipa::path(
    put, path = afd_wire::paths::LEASE_CONTEXT, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_put_context_checkpoint", summary = "Replace private run working notes",
    description = "Publishes a complete validated checkpoint under the current assignment and expected generation. Identical retries acknowledge one write; changed assignments or generations return 409 with superseded.",
    params(ContextPath), request_body = ContextPut,
    responses((status = 204, description = "Checkpoint saved"), (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED), (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND), (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE), (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE))
))]
pub(crate) async fn put<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(ContextPath {
        lease_id,
        branch_id,
    }): Path<ContextPath>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: ContextPut =
        read_strict_body(&body).map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let request = garde::Unvalidated::new(request)
        .validate()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    let checkpoint: afd_context::Checkpoint = serde_json::from_value(request.checkpoint.clone())
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    checkpoint
        .validate_document()
        .map_err(|_invalid| Refusal::malformed(MALFORMED))?;
    services
        .leases()
        .context_commit(
            runner.id(),
            &lease(&lease_id)?,
            &lease(&branch_id)?,
            request.fencing_token,
            request.expected_generation,
            checkpoint,
        )
        .await
        .map_err(Refusal::conflict_or_at(EVENT, conflict))?;
    Ok(http::StatusCode::NO_CONTENT.into_response())
}

fn conflict(error: &afd_fleet::Error) -> Option<&'static str> {
    (error.code() == afd_core::error_code::RUN_STALE_FENCING_TOKEN).then_some(SUPERSEDED)
}
