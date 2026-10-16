//! The shared runner poll serves exactly one explicitly accepted work kind.

mod input;

use crate::{
    auth::RunnerIdentity,
    services::{Leasing as _, Services},
};
use afd_http::handler::Refusal;
#[cfg(feature = "openapi")]
use afd_wire::lease::LeaseRequest;
use axum::{
    body::Bytes,
    extract::State,
    response::{IntoResponse as _, Response},
};
use http::{StatusCode, header};
use std::sync::Arc;

const EVENT: &str = "runner_lease_failed";

/// Place work under the authenticated host and explicit protocol selection.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::RUNNER_LEASES,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_lease",
    summary = "Poll for the next lease",
    description = concat!(
        "The poll a runner lives on. Answers either a lease to run or a ",
        "backoff to wait out. The claim, the money, the gates and the policy ",
        "are all decided before the answer is written. The body lists the ",
        "fleets whose sandboxes the runner holds, which are tried first. ",
        "accepted_kinds is required and selects fleet, authoring, or both. ",
    ),
    request_body = LeaseRequest,
    responses(
        (status = 200, description = "A lease to run, or a backoff to wait out", body = afd_wire::lease::LeaseResponse),
        (status = 400, description = "Invalid polling body or kind selection"),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request = input::read(&body)?;
    let body = services
        .leases()
        .lease(
            services.authoring(),
            runner.id(),
            &request.held,
            request.accepted,
            runner.is_degraded(),
            services.now(),
        )
        .await
        .map_err(Refusal::at(EVENT))?;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        body,
    )
        .into_response())
}
