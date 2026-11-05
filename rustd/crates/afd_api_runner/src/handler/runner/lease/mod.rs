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
use http::{HeaderMap, StatusCode, header};
use std::sync::Arc;

const EVENT: &str = "runner_lease_failed";

/// Place work under the authenticated host and explicit protocol selection.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = afd_wire::paths::RUNNER_LEASES,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_lease",
    summary = "Request the next work assignment",
    description = concat!(
        "Returns the next work assignment, or how long to wait before polling again. ",
        "Previously held fleet sandboxes are preferred. Required `accepted_kinds` selects ",
        "fleet work, private setup work, or both. Budget, approval, and policy checks ",
        "happen before assignment. An optional `Idempotency-Key` names one poll attempt: ",
        "send the same key again only when the earlier reply never arrived. A repeated ",
        "key frees the fleet assignment that reply carried and answers no work with ",
        "`retry_after_ms` 0. The next poll, under a new key, receives it again.",
    ),
    params(
        ("Idempotency-Key" = Option<String>, Header, description = "Optional UUIDv7 naming this poll attempt; reused only to repeat a poll whose reply was lost."),
    ),
    request_body = LeaseRequest,
    responses(
        (status = 200, description = "A lease to run, or a backoff to wait out", body = afd_wire::lease::LeaseResponse),
        (status = 400, description = "Invalid polling body, kind selection or `Idempotency-Key`"),
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
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, Refusal> {
    let key = input::poll_key(&headers)?;
    let request = input::read(&body)?;
    let body = services
        .leases()
        .lease(
            services.authoring(),
            runner.id(),
            afd_fleet::lease::Asked {
                held: &request.held,
                accepted: request.accepted,
                key: key.as_ref(),
            },
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
