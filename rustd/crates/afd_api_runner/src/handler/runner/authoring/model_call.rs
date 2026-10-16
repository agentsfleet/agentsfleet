//! The runner presents a lease capability, never a provider key or destination.

use crate::{auth::RunnerIdentity, services::Services};
use afd_authoring::ModelCallInput;
use afd_core::id::Uuid7;
use afd_http::handler::{Refusal, authoring_refusal};
#[cfg(feature = "openapi")]
use afd_wire::authoring::{ModelCall, ModelResult};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

/// Invokes the fixed provider only after a scoped durable dispatch reservation.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::AUTHORING_MODEL_CALLS,
    tag = afd_http::openapi::tag::RUNNERS, operation_id = "runner_call_authoring_model",
    summary = "Invoke the scoped authoring model", request_body = ModelCall,
    params(afd_http::openapi::path::Lease),
    responses((status = 200, body = ModelResult, description = "Durably recorded bounded model result"),
        (status = 400, description = "Invalid request"), (status = 401, description = "Unauthorized"),
        (status = 402, description = "Owner balance insufficient"), (status = 403, description = "Forbidden"),
        (status = 404, description = "Lease unavailable"), (status = 409, description = "Fenced, conflicting or unresolved dispatch"),
        (status = 413, description = "Envelope too large"), (status = 422, description = "Token limit exceeded"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 502, description = "Provider failed with known usage"),
        (status = 503, description = "Dependency unavailable"), (status = 504, description = "Attempt timed out"))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease =
        Uuid7::parse(&lease).map_err(|_invalid| Refusal::malformed("lease_id must be a UUIDv7"))?;
    let request: ModelCallInput = afd_core::json::object_from_slice(&body)
        .map_err(|_invalid| Refusal::malformed("Invalid authoring model-call body"))?;
    let result = services
        .authoring_models()
        .call(runner.id(), &lease, request)
        .await
        .map_err(authoring_refusal)?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(result),
    )
        .into_response())
}
