//! `POST /v1/runners/me/leases/{lease_id}/messages` — a fleet saying one line
//! to the thread it was asked from, before it answers.
//!
//! The lease plane proves the lease, finds the event's thread, counts the line
//! against the run's cap and masks the fleet's secret values
//! (`afd_fleet::lease::message`); the outbound poster then posts it under its
//! own marker, retried as an answer is (`afd_outbound::interim`). The runner
//! never holds the channel's credential: it sends text, and the daemon speaks.

use std::sync::Arc;

use afd_core::error_code;
use afd_http::handler::{Refusal, read_strict_body};
use afd_wire::message_verb::{
    MESSAGE_DELIVERY_DEADLINE, MESSAGE_MAX_BYTES, MESSAGES_PER_RUN_MAX, MessagePosted,
    MessageRequest,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use super::schedule::lease;
use crate::auth::RunnerIdentity;
use crate::services::{Leasing as _, Services};

/// The scoped event a message that failed is logged under.
const EVENT_FAILED: &str = "runner_message_failed";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed message body";

/// The refusal a text outside its bound earns.
const DETAIL_TEXT: &str = const_format::concatcp!(
    "text must be 1 to ",
    MESSAGE_MAX_BYTES,
    " bytes with no NUL"
);

/// The `current_state` an event with no thread names.
const STATE_NO_THREAD: &str = "no_thread";

/// The `current_state` a run at its message cap names.
const STATE_AT_CAPACITY: &str = "at_capacity";

/// A post that outlasted [`MESSAGE_DELIVERY_DEADLINE`], answered undelivered.
const EVENT_DEADLINE: &str = "runner_message_deadline";

/// What [`handle`] documents, its caps spelled from their constants.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = const_format::concatcp!(
    "Posts one line to the thread the leased event came from, before the ",
    "run answers, with the fleet's secret values masked. The daemon holds ",
    "the channel's credential and posts; the runner sends text. A run may ",
    "post at most ",
    MESSAGES_PER_RUN_MAX,
    " messages, refused past that with `UZ-RUN-020`. An event from no ",
    "thread, such as an API steer, a webhook or a schedule, is refused with ",
    "`UZ-RUN-019`. Both are 409s naming their `current_state`. `delivered` ",
    "is false when the channel refused the line, or stayed unreachable ",
    "through every retry within ",
    MESSAGE_DELIVERY_DEADLINE.as_secs(),
    " seconds. Takes no `Idempotency-Key`: each line is its own post. ",
);

/// Posts one line to the event's thread.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/messages",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_post_message",
    summary = "Post a message to the event's thread",
    description = DESCRIPTION,
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
    let interim = services
        .leases()
        .message(runner.id(), lease(&lease_id)?, &request, services.now())
        .await
        .map_err(refused)?;
    let part = interim.part;
    let posting = services.interjector().interject(interim);
    let delivered = tokio::time::timeout(MESSAGE_DELIVERY_DEADLINE, posting)
        .await
        .unwrap_or_else(|_elapsed| {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let lease_id = lease_id.as_str();
            let error_code = error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
            tracing::warn!(error_code, lease_id, part, event = EVENT_DEADLINE);
            false
        });
    Ok(Json(MessagePosted { delivered }).into_response())
}

/// A message the lease plane refused, as the runner reads it: each 409 names
/// the state that forbade it.
fn refused(error: afd_fleet::Error) -> Refusal {
    let code = error.code();
    if code == error_code::MESSAGE_NO_CHANNEL {
        Refusal::conflict_at(EVENT_FAILED, STATE_NO_THREAD)(error)
    } else if code == error_code::MESSAGE_LIMIT_REACHED {
        Refusal::conflict_at(EVENT_FAILED, STATE_AT_CAPACITY)(error)
    } else {
        Refusal::at(EVENT_FAILED)(error)
    }
}
