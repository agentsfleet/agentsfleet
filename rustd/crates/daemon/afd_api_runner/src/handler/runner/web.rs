//! Single-send searches and extracts for a held run on a public fleet.

use std::sync::Arc;

use af_wire::web::{ExtractRequest, SearchRequest, WEB_DEADLINE};
#[cfg(feature = "openapi")]
use af_wire::web::{ExtractResponse, SearchResponse};
use afd_http::envelope::ProblemResponse;
use afd_http::handler::{Refusal, read_strict_body};
use afd_http::request_id::RequestId;
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::{HeaderValue, header::RETRY_AFTER};
use tokio::time::Instant;

use super::schedule::run;
use crate::auth::RunnerIdentity;
use crate::services::{RunPlane as _, RunnerPlane};

/// Search the web under the run's current fence and fleet network policy.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = af_wire::paths::RUN_SEARCHES, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_web_search", summary = "Search the web for a public fleet",
    description = "Submit this request once. Each vendor dispatch reserves a physical attempt. A lost response leaves the outcome unknown. HTTP 503 includes Retry-After when all vendors are cooling down.",
    request_body = SearchRequest, params(afd_http::openapi::path::Run),
    responses(
        (status = 200, description = "Results from the first working vendor", body = SearchResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 502, description = "Search vendors failed"),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn search<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let deadline = Instant::now() + WEB_DEADLINE;
    let request: SearchRequest =
        read_strict_body(&body).map_err(|_| Refusal::malformed("Malformed web search body"))?;
    let input = garde::Unvalidated::new(request)
        .validate()
        .map_err(|_| Refusal::malformed("Invalid search query or call number"))?;
    let results = services
        .runs()
        .web_search(runner.id(), run(&run_id)?, &input, deadline)
        .await
        .map_err(refused)?;
    Ok(Json(results).into_response())
}

/// Read a public web page using the platform reader.
#[cfg_attr(feature = "openapi", utoipa::path(
    post, path = af_wire::paths::RUN_EXTRACTS, tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_web_extract", summary = "Read a page for a public fleet",
    description = "Submit once. A temporary reader failure permits one local read only if the problem explicitly carries local_read_permitted: true.",
    request_body = ExtractRequest, params(afd_http::openapi::path::Run),
    responses(
        (status = 200, description = "Page content as Markdown", body = ExtractResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 502, description = "Page unavailable, reader unavailable, or invalid reader response"),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn extract<D: RunnerPlane>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let deadline = Instant::now() + WEB_DEADLINE;
    let request: ExtractRequest =
        read_strict_body(&body).map_err(|_| Refusal::malformed("Malformed web extract body"))?;
    let input = garde::Unvalidated::new(request)
        .validate()
        .map_err(|_| Refusal::malformed("Invalid page URL or call number"))?;
    let page = services
        .runs()
        .web_extract(runner.id(), run(&run_id)?, &input, deadline)
        .await
        .map_err(refused)?;
    Ok(Json(page).into_response())
}

/// Only the service's live authorization can issue local read permission.
fn refused(error: afd_fleet::Error) -> Refusal {
    let Some(failure) = error.web_failure() else {
        return Refusal::at("runner_web_failed")(error);
    };
    let id = RequestId::current();
    let mut reply = if failure.local_read_permitted() {
        ProblemResponse::extract_fallback(failure.detail(), id)
    } else {
        ProblemResponse::new(error.code(), failure.detail(), id)
    }
    .into_response();
    if let Some(wait) = failure.retry_after() {
        let seconds = wait
            .as_secs()
            .saturating_add(u64::from(wait.subsec_nanos() != 0));
        reply
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from(seconds));
    }
    reply.into()
}
