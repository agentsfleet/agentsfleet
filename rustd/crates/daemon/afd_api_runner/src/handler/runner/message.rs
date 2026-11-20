//! `POST /v1/runners/me/runs/{run_id}/messages` — a fleet saying one line
//! to the thread it was asked from, before it answers.
//!
//! The run plane proves the run, finds the event's thread, numbers the line
//! and masks the fleet's secret values (`afd_fleet::run::message`); the
//! outbound poster then posts it under its own marker, retried as an answer is
//! (`afd_outbound::interim`). The runner never holds the channel's credential:
//! it sends text, and the daemon speaks. Neither the line's length nor how
//! many a run sends is capped here: the channel refuses what it cannot take,
//! and the runner reads `delivered: false`.

use std::sync::Arc;

use af_core::error_code;
use af_wire::message_verb::{MESSAGE_DELIVERY_DEADLINE, MessagePosted, MessageRequest};
use afd_http::handler::{Refusal, read_strict_body};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use garde::Validate as _;

use super::schedule::run;
use crate::auth::RunnerIdentity;
use crate::services::{RunPlane as _, Services};

/// The scoped event a message that failed is logged under.
const EVENT_FAILED: &str = "runner_message_failed";

/// The refusal a body this verb cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed message body";

/// The refusal a text that is empty or holds NUL earns.
const DETAIL_TEXT: &str = "text must be nonempty with no NUL";

/// The `current_state` an event with no thread names.
const STATE_NO_THREAD: &str = "no_thread";

/// A post that outlasted [`MESSAGE_DELIVERY_DEADLINE`], answered undelivered.
const EVENT_DEADLINE: &str = "runner_message_deadline";

/// What [`handle`] documents, its deadline spelled from its constant.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = const_format::concatcp!(
    "Posts to the event's original thread with secrets masked. The server holds the ",
    "channel credential. Events without a thread return 409 `UZ-RUN-019` with ",
    "`current_state`. `delivered` is false when the channel refuses the message or ",
    "retries fail within ",
    MESSAGE_DELIVERY_DEADLINE.as_secs(),
    " seconds. Each request posts a new message; `Idempotency-Key` is unsupported.",
);

/// Posts one line to the event's thread.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUN_MESSAGES,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_post_message",
    summary = "Post a message to the event's thread",
    description = DESCRIPTION,
    request_body = MessageRequest<'_>,
    params(afd_http::openapi::path::Run),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = MessagePosted),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(run_id): Path<String>,
    body: Bytes,
) -> Result<Response, Refusal> {
    let request: MessageRequest<'_> =
        read_strict_body(&body).map_err(|_unreadable| Refusal::malformed(DETAIL_MALFORMED))?;
    request
        .validate()
        .map_err(|_report| Refusal::malformed(DETAIL_TEXT))?;
    let interim = services
        .runs()
        .message(runner.id(), run(&run_id)?, &request)
        .await
        .map_err(refused)?;
    let part = interim.part;
    let posting = services.interjector().interject(interim);
    let delivered = tokio::time::timeout(MESSAGE_DELIVERY_DEADLINE, posting)
        .await
        .unwrap_or_else(|_elapsed| {
            // Hoisted: see the `tracing` note in the workspace Cargo.toml.
            let run_id = run_id.as_str();
            let error_code = error_code::CONNECTOR_VENDOR_DEADLINE.as_str();
            tracing::warn!(error_code, run_id, part, event = EVENT_DEADLINE);
            false
        });
    Ok(Json(MessagePosted { delivered }).into_response())
}

/// A message the run plane refused, as the runner reads it: the 409 for an
/// event with no thread names the state that forbade it.
fn refused(error: afd_fleet::Error) -> Refusal {
    if error.code() == error_code::MESSAGE_NO_CHANNEL {
        Refusal::conflict_at(EVENT_FAILED, STATE_NO_THREAD)(error)
    } else {
        Refusal::at(EVENT_FAILED)(error)
    }
}
