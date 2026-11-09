//! Stream metadata and state belong to the gateway; the common guard already proved the principal.

use afd_http::{
    auth::RunnerIdentity,
    services::{RunnerStreams as _, Services},
};
use axum::{
    extract::{Request, State},
    response::Response,
};
use std::sync::Arc;

/// Open one duplex request of the runner's authenticated native session.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUNNER_STREAM,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_stream",
    summary = "Open a runner session stream",
    description = "Open control first with the runner bearer credential. After prepared, confirm and welcome, open work, reports and activity on that same native connection using the returned session identifier. Bodies contain length-prefixed JSON frames. Requires the configured private HTTP/3 or HTTP/2 listener; ordinary API connections cannot open a session. Reports retain lease-keyed idempotence and take no Idempotency-Key.",
    request_body(content = String, content_type = "application/octet-stream"),
    params(
        ("agentsfleet-runner-protocol" = String, Header, description = "agentsfleet.runner.v1"),
        ("agentsfleet-runner-kind" = String, Header, description = "control, work, reports or activity"),
        ("agentsfleet-runner-session" = Option<String>, Header, description = "Previous owner on control reconnect; welcome owner on secondary requests"),
    ),
    responses(
        (status = 200, description = "Length-prefixed session frames", body = String, content_type = "application/octet-stream"),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 409, description = "The previous connection no longer owns this runner"),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    request: Request,
) -> Response {
    services.runner_streams().open(runner, request).await
}
