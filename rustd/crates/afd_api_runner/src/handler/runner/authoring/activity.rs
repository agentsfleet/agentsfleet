//! A proposal is acknowledged only after the immutable candidate receipt commits.

use std::sync::Arc;

use afd_authoring::ActivityInput;
use afd_core::id::Uuid7;
#[cfg(feature = "openapi")]
use afd_wire::authoring::{ActivityAccepted, ActivityRequest};
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::StatusCode;

use crate::{auth::RunnerIdentity, services::Services};
use afd_http::handler::{Refusal, authoring_refusal};

/// Applies the one fixed proposal tool, under live membership and attempt fences.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::AUTHORING_ACTIVITY,
    tag = afd_http::openapi::tag::RUNNERS, operation_id = "runner_publish_authoring_activity",
    summary = "Submit private authoring activity", request_body = ActivityRequest,
    params(afd_http::openapi::path::Lease),
    responses((status = 202, body = ActivityAccepted, description = "Durable proposal receipt or progress acknowledgement"),
        (status = 400, description = "Invalid activity"), (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"), (status = 404, description = "Lease unavailable"),
        (status = 409, description = "Lost lease or conflicting activity"),
        (status = 412, description = "Candidate base changed"), (status = 413, description = "Envelope too large"),
        (status = 429, description = "Too many requests"), (status = 500, description = "Internal failure"),
        (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease =
        Uuid7::parse(&lease).map_err(|_invalid| Refusal::malformed("lease_id must be a UUIDv7"))?;
    // Definition keys and proposed text are private authored data. The shared
    // object parser refuses malformed/unknown input without logging those keys.
    let request: ActivityInput = afd_core::json::object_from_slice(&body)
        .map_err(|_invalid| Refusal::malformed("Invalid authoring activity body"))?;
    let accepted = services
        .authoring()
        .activity(runner.id(), &lease, request)
        .await
        .map_err(authoring_refusal)?;
    Ok((StatusCode::ACCEPTED, Json(accepted)).into_response())
}
