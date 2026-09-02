//! The six device-flow verbs: four here, and the two a browser calls in
//! [`dashboard`].

pub(crate) mod dashboard;

use std::borrow::Cow;
use std::sync::Arc;

use afd_tenant::session::input::Opening;
use afd_tenant::session::{Cancelled, SessionStatus};
use afd_wire::auth::{
    DeleteAllSessionsResponse, OpenSessionRequest, OpenSessionResponse, PollSessionResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::auth::DashboardIdentity;
use crate::handler::{malformed, refuse};
use crate::request_id::RequestId;
use crate::services::{DeviceFlow as _, Services};

/// The scoped events each verb's failures are logged under.
const EVENT_OPEN: &str = "auth_session_open_failed";
const EVENT_POLL: &str = "auth_session_poll_failed";
const EVENT_APPROVE: &str = "auth_session_approve_failed";
const EVENT_VERIFY: &str = "auth_session_verify_failed";
const EVENT_CANCEL: &str = "auth_session_cancel_failed";
const EVENT_CANCEL_ALL: &str = "auth_sessions_cancel_all_failed";

/// The refusals a body this daemon cannot read earns, one per verb.
///
/// Separate sentences rather than one shared "malformed body", because each
/// names the fields the verb actually takes — which is the difference between a
/// client author finding the typo and opening a ticket.
const DETAIL_OPEN_BODY: &str = "Malformed JSON or missing public_key/token_name";
const DETAIL_APPROVE_BODY: &str = "Malformed approve payload";
const DETAIL_VERIFY_BODY: &str = "Malformed verify payload";

/// `POST /v1/auth/sessions` — the command line opens a login.
///
/// Unauthenticated, and it has to be: the caller is a terminal that holds no
/// credential yet, which is the whole reason the flow exists. What bounds it is
/// the session's five-minute life and the ceilings on every field it may store.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/auth/sessions",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "create_auth_session",
    summary = "Start a command-line login session",
    description = concat!(
        "Starts a command-line sign-in session that lasts 5 minutes. The ",
        "response includes a URL for approval in a browser. No access token ",
        "is required. Rate limits may return 429. ",
    ),
    request_body = OpenSessionRequest,
    params(
    ),
    responses(
        (status = 201, description = afd_http::openapi::CREATED, body = OpenSessionResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn open<D: Services>(State(services): State<Arc<D>>, body: Bytes) -> Response {
    let Ok(request) = afd_core::json::object_from_slice::<OpenSessionRequest<'_>>(&body) else {
        return malformed(DETAIL_OPEN_BODY);
    };
    let opening = match Opening::parse(&request.public_key, &request.token_name) {
        Ok(opening) => opening,
        Err(error) => return refuse(&error, EVENT_OPEN),
    };

    match services.sessions().open(&opening, services.now()).await {
        Ok(opened) => {
            let request_id = RequestId::mint();
            (
                StatusCode::CREATED,
                Json(OpenSessionResponse {
                    session_id: Cow::Owned(opened.session_id),
                    login_url: Cow::Owned(opened.login_url),
                    request_id: Cow::Owned(request_id.as_str().to_owned()),
                }),
            )
                .into_response()
        }
        Err(error) => refuse(&error, EVENT_OPEN),
    }
}

/// `GET /v1/auth/sessions/{session_id}` — where the login has got to.
///
/// Never returns ciphertext. The id is the only thing presented, so everything
/// this answers is readable by whoever holds it; the sealed credential is
/// released by `/verify` alone, against a code that travelled a second channel.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = "/v1/auth/sessions/{session_id}",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "poll_auth_session",
    summary = "Read a sign-in session",
    description = concat!(
        "Returns the current sign-in status. No access token is required. ",
        "Expired, used, or cancelled sessions return 410 with a stable error ",
        "code. ",
    ),
    params(
        afd_http::openapi::path::Session,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = PollSessionResponse),
        (status = 401, description = afd_http::openapi::SESSION_EXPIRED),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn poll<D: Services>(
    State(services): State<Arc<D>>,
    Path(session_id): Path<String>,
) -> Response {
    match services.sessions().poll(&session_id).await {
        Ok(waiting) => Json(PollSessionResponse {
            status: Cow::Borrowed(status_name(waiting.status)),
            cli_public_key: Cow::Owned(waiting.cli_public_key),
            token_name: Cow::Owned(waiting.token_name),
            expires_at_ms: waiting.expires_at_ms,
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT_POLL),
    }
}

/// `DELETE /v1/auth/sessions/{session_id}` — a person cancels their own login.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/auth/sessions/{session_id}",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "delete_auth_session",
    summary = "Explicit cancel of a single login session",
    description = concat!(
        "Used by the dashboard's \"cancel this login\" button and by the future ",
        "sessions surface. Transitions the named session to `aborted` with ",
        "`reason=\"explicit_cancel\"`. The Clerk JWT MUST match the session's ",
        "`clerk_user_id` (set on `PATCH /v1/auth/sessions/{id}/approve`); ",
        "otherwise 403. ",
    ),
    params(
        afd_http::openapi::path::Session,
    ),
    responses(
        (status = 204, description = afd_http::openapi::NO_CONTENT),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = "The session was started by somebody else"),
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn delete_one<D: Services>(
    State(services): State<Arc<D>>,
    dashboard: DashboardIdentity,
    Path(session_id): Path<String>,
) -> Response {
    match services
        .sessions()
        .cancel(&session_id, dashboard.subject())
        .await
    {
        Ok(cancelled) => {
            // Logged on the TRANSITION only. A repeat of a cancel is idempotent
            // and records nothing, so an audit reader counting aborts counts
            // sessions rather than requests.
            if cancelled == Cancelled::Now {
                tracing::debug!(event = "auth_session_cancelled");
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Err(error) => refuse(&error, EVENT_CANCEL),
    }
}

/// `DELETE /v1/auth/sessions/all` — abort every login this person holds.
#[cfg_attr(feature = "openapi", utoipa::path(
    delete,
    path = "/v1/auth/sessions/all",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "delete_all_auth_sessions",
    summary = "Bulk-abort every in-flight login session for the caller",
    description = concat!(
        "Caller-scoped bulk delete. Enumerates every session with the ",
        "caller's `clerk_user_id` whose status is `pending` or ",
        "`verification_pending` and transitions each to `aborted` with ",
        "`reason=\"explicit_cancel\"`. Does NOT revoke already-minted JWTs — ",
        "Clerk revocation is a separate problem; an active CLI continues to ",
        "work until its short-lived JWT expires. This endpoint clears in- ",
        "flight login sessions only. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = DeleteAllSessionsResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn delete_all<D: Services>(
    State(services): State<Arc<D>>,
    dashboard: DashboardIdentity,
) -> Response {
    match services.sessions().cancel_all(dashboard.subject()).await {
        Ok(aborted) => {
            let aborted_count = aborted.len();
            tracing::debug!(aborted_count, event = "auth_sessions_bulk_cancelled");
            Json(DeleteAllSessionsResponse { aborted_count }).into_response()
        }
        Err(error) => refuse(&error, EVENT_CANCEL_ALL),
    }
}

/// The wire spelling of a state a poll may report.
///
/// Only the two non-terminal states reach here: the service answers the other
/// three as refusals, so there is no arm below that could put `consumed` on a
/// 200. The match is still total, because a state added to the store must be
/// answered here rather than defaulting to whichever spelling came first.
const fn status_name(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Pending => "pending",
        SessionStatus::VerificationPending => "verification_pending",
        SessionStatus::Consumed => "consumed",
        SessionStatus::Expired => "expired",
        SessionStatus::Aborted => "aborted",
    }
}
