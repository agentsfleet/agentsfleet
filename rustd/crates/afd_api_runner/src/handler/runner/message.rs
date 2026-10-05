//! `POST /v1/runners/me/leases/{lease_id}/messages` — a fleet saying one line
//! to the thread it was asked from, before it answers.
//!
//! The lease plane proves the lease, finds the event's thread, counts the line
//! against the run's cap and masks the fleet's secret values
//! (`afd_fleet::lease::message`); the outbound poster then posts it under its
//! own marker, retried as an answer is (`afd_outbound::interim`). The runner
//! never holds the channel's credential: it sends text, and the daemon speaks.

use std::sync::Arc;

use afd_http::handler::{Refusal, parse_id, read_strict_body};
use afd_wire::message_verb::{MessagePosted, MessageRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use crate::auth::RunnerIdentity;
use crate::services::{Leasing as _, Services};

/// The scoped event a message that failed is logged under.
const EVENT_FAILED: &str = "runner_message_failed";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed message body";

/// The refusal a text outside its bound earns.
const DETAIL_TEXT: &str = "text must be 1 to 4096 bytes with no NUL";

/// The refusal a lease path segment that is not an identifier earns.
const DETAIL_LEASE_ID: &str = "lease_id must be a valid UUIDv7";

/// Posts one line to the event's thread.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/messages",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_post_message",
    summary = "Post a message to the event's thread",
    description = concat!(
        "Posts one line to the thread the leased event came from, before the ",
        "run answers, with the fleet's secret values masked. The daemon holds ",
        "the channel's credential and posts; the runner sends text. A run may ",
        "post at most 8 messages, refused past that with `UZ-RUN-020`. An ",
        "event from no thread, such as an API steer, a webhook or a schedule, ",
        "is refused with `UZ-RUN-019`. `delivered` is false when the channel ",
        "refused the line or stayed unreachable through every retry. ",
    ),
    request_body = MessageRequest,
    params(afd_http::openapi::path::Lease),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = MessagePosted),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: MessageRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    request
        .validate()
        .map_err(|_report| Refusal::malformed(DETAIL_TEXT))?;
    let lease = parse_id(&lease_id, DETAIL_LEASE_ID)?;
    let interim = services
        .leases()
        .message(runner.id(), lease, &request, services.now())
        .await
        .map_err(Refusal::at(EVENT_FAILED))?;
    let delivered = services.interjector().interject(interim).await;
    Ok(Json(MessagePosted { delivered }).into_response())
}
