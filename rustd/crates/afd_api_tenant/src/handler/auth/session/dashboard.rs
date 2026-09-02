//! The two verbs a signed-in browser calls, not the command line.
//!
//! Split out of [`super`], which is at the length cap. These are the only
//! device-flow verbs behind [`DashboardIdentity`]: the handshake in [`super`]
//! is unauthenticated by construction — the command line has no credential yet,
//! which is the whole point of it asking — and the person approving is the one
//! who already has one.

use std::borrow::Cow;
use std::sync::Arc;

use afd_observability::Telemetry;
use afd_tenant::session::Fingerprint;
use afd_tenant::session::input::{Approval, Code};
use afd_wire::auth::{
    ApproveSessionRequest, ApproveSessionResponse, VerifySessionRequest, VerifySessionResponse,
};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::DashboardIdentity;
use crate::client::Origin;
use crate::handler::{malformed, refuse};
use crate::request_id::RequestId;
use crate::services::{DeviceFlow as _, Services};

use super::{DETAIL_APPROVE_BODY, DETAIL_VERIFY_BODY, EVENT_APPROVE, EVENT_VERIFY};

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
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
        (status = 401, description = afd_http::openapi::SESSION_EXPIRED),
        (status = 410, description = afd_http::openapi::GONE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
