//! Own-lease report uses the shared daemon authoring boundary.

use crate::{auth::RunnerIdentity, services::Services};
use afd_core::id::Uuid7;
use afd_http::handler::{Refusal, authoring_refusal};
use afd_wire::authoring::Report;
#[cfg(feature = "openapi")]
use afd_wire::authoring::Reported;
use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

/// Record authoring accounting and terminal state under the authenticated runner identity.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = afd_wire::paths::AUTHORING_REPORTS,
    tag = afd_http::openapi::tag::RUNNERS, operation_id = "runner_authoring_report",
    summary = "Record authoring accounting and terminal state", request_body = Report,
    params(afd_http::openapi::path::Lease),
    responses((status = 200, body = Reported, description = "Durable acknowledgement"),
        (status = 400, description = "Invalid request"), (status = 401, description = "Unauthorized"),
        (status = 402, description = "Wallet coverage insufficient"), (status = 403, description = "Forbidden"),
        (status = 404, description = "Lease unavailable"), (status = 409, description = "Lost lease or replay conflict"),
        (status = 413, description = "Envelope too large"), (status = 429, description = "Too many requests"),
        (status = 500, description = "Internal failure"), (status = 503, description = "Dependency unavailable"))
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let lease =
        Uuid7::parse(&lease).map_err(|_invalid| Refusal::malformed("lease_id must be a UUIDv7"))?;
    let request: Report = afd_core::json::object_from_slice(&body)
        .map_err(|_invalid| Refusal::malformed("Invalid authoring report body"))?;
    let result = services
        .authoring()
        .report(runner.id(), &lease, request)
        .await
        .map_err(authoring_refusal)?;
    Ok((
        StatusCode::OK,
        [(header::CACHE_CONTROL, "no-store")],
        Json(result),
    )
        .into_response())
}
