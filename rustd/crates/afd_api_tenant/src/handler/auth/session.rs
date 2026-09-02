//! The six device-flow verbs.

use std::borrow::Cow;
use std::sync::Arc;

use afd_tenant::session::input::{Approval, Code, Opening};
use afd_tenant::session::{Cancelled, Fingerprint, SessionStatus};
use afd_wire::auth::{
    ApproveSessionRequest, ApproveSessionResponse, DeleteAllSessionsResponse, OpenSessionRequest,
    OpenSessionResponse, PollSessionResponse, VerifySessionRequest, VerifySessionResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use afd_observability::Telemetry;

use crate::auth::DashboardIdentity;
use crate::client::Origin;
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
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 500, description = afd_http::openapi::INTERNAL),
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

/// `PATCH /v1/auth/sessions/{session_id}/approve` — a person clicked Approve.
///
/// [`DashboardIdentity`] and not a person: an `agt_t` api-key resolves to the
/// same human with the same capabilities, and it must still not approve a
/// device login, because the flow's entire guarantee is that somebody looked at
/// a screen. The narrowing is in the signature, so there is no arm here to
/// forget it in.
#[cfg_attr(feature = "openapi", utoipa::path(
    patch,
    path = "/v1/auth/sessions/{session_id}/approve",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "approve_auth_session",
    summary = "Approve a sign-in session",
    description = concat!(
        "Approves a pending command-line sign-in session. The dashboard sends ",
        "encrypted sign-in data and a six-digit verification code. A second ",
        "approval for the same session returns 409 `UZ-AUTH-015`. ",
    ),
    request_body = ApproveSessionRequest,
    params(
        afd_http::openapi::path::Session,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ApproveSessionResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 409, description = afd_http::openapi::CONFLICT),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn approve<D: Services>(
    State(services): State<Arc<D>>,
    dashboard: DashboardIdentity,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(request) = afd_core::json::object_from_slice::<ApproveSessionRequest<'_>>(&body) else {
        return malformed(DETAIL_APPROVE_BODY);
    };
    let approval = match Approval::parse(
        &request.dashboard_public_key,
        &request.ciphertext,
        &request.nonce,
        &request.verification_code,
    ) {
        Ok(approval) => approval,
        Err(error) => return refuse(&error, EVENT_APPROVE),
    };

    match services
        .sessions()
        .approve(&session_id, &approval, dashboard.subject(), services.now())
        .await
    {
        Ok(()) => {
            let request_id = RequestId::mint();
            // The moment a person completes a sign-in. Reported HERE rather
            // than at the verify beside it: the verify is the terminal
            // collecting its credential and knows no subject, and attributing
            // a login to nobody is the same as not reporting it.
            services.analytics().report(&Telemetry::AuthLoginCompleted {
                actor: dashboard.subject().to_owned(),
                session_id: session_id.clone(),
                request_id: request_id.as_str().to_owned(),
            });
            Json(ApproveSessionResponse {
                request_id: Cow::Owned(request_id.as_str().to_owned()),
            })
            .into_response()
        }
        Err(error) => refuse(&error, EVENT_APPROVE),
    }
}

/// `POST /v1/auth/sessions/{session_id}/verify` — the code is presented.
///
/// Unauthenticated, and the code IS the credential. The origin is digested into
/// a fingerprint so a dropped reply can be asked for again by whoever asked
/// first — and by nobody else.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/auth/sessions/{session_id}/verify",
    tag = afd_http::openapi::tag::AUTHENTICATION,
    operation_id = "verify_auth_session",
    summary = "Verify a sign-in session",
    description = concat!(
        "Checks the six-digit code and returns encrypted sign-in data. The ",
        "code can be used once. A retry from the same client within 60 ",
        "seconds returns the same response. Later retries return 410 `UZ- ",
        "AUTH-012`. `UZ-AUTH-011` means the code did not match. The fifth ",
        "failed attempt ends the session. `UZ-AUTH-018` means the code was ",
        "not six digits. `UZ-AUTH-013` means the session ended. `UZ-AUTH-014` ",
        "means approval is pending. `UZ-AUTH-006` means the session expired ",
        "after 5 minutes. ",
    ),
    request_body = VerifySessionRequest,
    params(
        afd_http::openapi::path::Session,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = VerifySessionResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn verify<D: Services>(
    State(services): State<Arc<D>>,
    origin: Origin,
    Path(session_id): Path<String>,
    body: Bytes,
) -> Response {
    let Ok(request) = afd_core::json::object_from_slice::<VerifySessionRequest<'_>>(&body) else {
        return malformed(DETAIL_VERIFY_BODY);
    };
    // Shape first, before anything is computed over it: a code that cannot be
    // right costs no message authentication code and teaches nothing.
    let code = match Code::parse(&request.verification_code) {
        Ok(code) => code,
        Err(error) => return refuse(&error, EVENT_VERIFY),
    };
    let fingerprint = Fingerprint::of(origin.address.as_str(), &origin.user_agent, &session_id);

    match services
        .sessions()
        .verify(&session_id, &code, &fingerprint, services.now())
        .await
    {
        Ok(redeemed) => {
            // Whether this was the first redemption or a repeat rides the log
            // and never the wire — see `afd_tenant::session::Redeemed`.
            tracing::debug!(
                repeated = redeemed.repeated,
                event = "auth_session_verified",
            );
            Json(VerifySessionResponse {
                dashboard_public_key: Cow::Owned(redeemed.dashboard_public_key),
                ciphertext: Cow::Owned(redeemed.ciphertext),
                nonce: Cow::Owned(redeemed.nonce),
            })
            .into_response()
        }
        Err(error) => refuse(&error, EVENT_VERIFY),
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
        (status = 404, description = afd_http::openapi::NOT_FOUND),
        (status = 500, description = afd_http::openapi::INTERNAL),
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
