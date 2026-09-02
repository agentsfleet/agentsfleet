//! `POST /v1/runners/me/leases/{lease_id}/activity` — live-tail frames.
//!
//! # 202, and why not 200
//!
//! The reply is an acknowledgement that the frames were RECEIVED, not that they
//! were published — the publish is best-effort and happens whether or not
//! anybody is listening on the channel. A 200 would imply the daemon is
//! reporting on the outcome of the work, and the runner would have no way to
//! act on a promise the daemon never made. The body is `{"ok":true}`, which is
//! what `service_activity.zig` answers: the first port of this verb dropped it
//! and answered a bare status, and the document gate is what noticed.
//!
//! # The only hard check is authorization
//!
//! A lease that does not resolve, or resolves to another runner, is a 404 — a
//! runner must not be able to write into a fleet's live tail by naming a lease
//! id it does not hold. Everything after that check is cosmetic and cannot fail
//! the request.

use std::sync::Arc;

use afd_wire::activity::{ActivityAccepted, ActivityRequest};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed forward is logged under.
const EVENT: &str = "runner_activity_failed";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed activity body";

/// Forwards one batch of live-tail frames.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/activity",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_publish_activity",
    summary = "Publish live-tail activity frames",
    description = concat!(
        "Frames for the workspace event stream. Answers 202 rather than 200: ",
        "the frame is accepted for publication, and a subscriber reading it ",
        "is a separate event from this call returning. ",
    ),
    request_body = ActivityRequest,
    params(
        afd_http::openapi::path::Lease,
    ),
    responses(
        (status = 202, description = afd_http::openapi::ACCEPTED, body = ActivityAccepted),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Response {
    // Borrowed out of `body`: every frame's text and arguments are re-emitted
    // into the published payload unchanged, so owning them would copy a run's
    // entire output stream one chunk at a time.
    let Ok(request) = afd_core::json::object_from_slice::<ActivityRequest<'_>>(&body) else {
        return crate::envelope::ProblemResponse::new(
            afd_core::error_code::INVALID_REQUEST,
            DETAIL_MALFORMED,
            crate::request_id::RequestId::mint(),
        )
        .into_response();
    };

    match services
        .leases()
        .activity(runner.id(), &lease_id, &request.frames)
        .await
    {
        Ok(()) => (StatusCode::ACCEPTED, Json(ActivityAccepted { ok: true })).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
