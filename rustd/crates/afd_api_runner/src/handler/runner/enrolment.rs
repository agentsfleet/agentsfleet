//! `POST /v1/runners` — enrol a host and mint its credential.
//!
//! The one verb on the runner family that a runner cannot call. It is
//! `Guard::Bearer` and requires `runner:enroll`, held independently of
//! `runner:read` and `runner:write` because it is uniquely dangerous: the host
//! it creates then receives every tenant's inline secrets, so the grant is
//! separately withheld and separately revoked rather than folded into an admin
//! rung nobody could refuse.
//!
//! # The token is revealed once
//!
//! The row stores a digest. The value itself exists for exactly as long as this
//! response takes to write, and `afd_fleet`'s `Minted` zeroes it on drop — so
//! the window is this function, and nothing in it logs the payload.

use std::borrow::Cow;
use std::sync::Arc;

use afd_runner::Enrolled;
use afd_wire::runner::{AssignedPolicy, RegisterRequest, RegisterResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::StatusCode;

use crate::envelope::ProblemResponse;
use crate::handler::refuse;
use crate::request_id::RequestId;
use crate::services::Services;

/// The scoped event a failed enrolment is logged under.
const EVENT: &str = "runner_enrolment_failed";

/// `register.zig`'s refusal for an absent body.
const DETAIL_BODY_REQUIRED: &str = "Request body required";

/// `register.zig`'s refusal for a body it could not read, naming the shape.
///
/// Pinned byte-for-byte: an operator enrolling a host by hand reads this
/// sentence to find out what they got wrong, which makes it behaviour.
const DETAIL_MALFORMED_BODY: &str = "Malformed JSON body (host_id, assigned_policy{sandbox_tier, \
                                     network_policy, registry_allowlist[], worker_count}, labels[])";

/// Enrols a host, answering its identity and its one-time credential.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners",
    tag = afd_http::openapi::tag::FLEET,
    operation_id = "register_runner",
    summary = "Register a runner",
    description = concat!(
        "Enrolls a runner into the fleet and assigns its policy. Requires ",
        "an existing operator credential (Clerk JWT or `agt_t` API key ",
        "with admin role); there is no enrollment token. Mints a durable ",
        "`agt_r` runner token, returned once, and stores only its SHA-256 ",
        "hash. The host applies the policy assigned here and never declares ",
        "its own. ",
    ),
    responses(
        (status = 201, description = "Runner registered; the bearer token is returned exactly once", body = RegisterResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(State(services): State<Arc<D>>, body: Bytes) -> Response {
    if body.is_empty() {
        return reject(DETAIL_BODY_REQUIRED);
    }
    // Strict, where the heartbeat is lenient, and the asymmetry is the point:
    // an operator typing an enrolment needs to be told their request was wrong,
    // while a runner beating every ten seconds needs its liveness to land
    // whatever its build sends. `RegisterRequest` carries
    // `deny_unknown_fields`, so a misspelled key is refused here rather than
    // silently assigning a policy nobody asked for.
    let Ok(request) = afd_core::json::object_from_slice::<RegisterRequest<'_>>(&body) else {
        return reject(DETAIL_MALFORMED_BODY);
    };

    match services.runners().register(&request, services.now()).await {
        Ok(enrolled) => created(&enrolled, &request.assigned_policy),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The 201, carrying the credential this response exists to reveal.
///
/// The assignment echoed back is the one AS STORED — the worker count clamped
/// into the shared bounds — so what the operator reads is what the host will
/// apply, rather than what they asked for.
fn created(enrolled: &Enrolled, requested: &AssignedPolicy<'_>) -> Response {
    let stored = AssignedPolicy {
        worker_count: enrolled.worker_count,
        ..requested.clone()
    };
    (
        StatusCode::CREATED,
        Json(RegisterResponse {
            runner_id: Cow::Borrowed(enrolled.runner_id.as_str()),
            runner_token: Cow::Borrowed(enrolled.token.expose()),
            assigned_policy: stored,
        }),
    )
        .into_response()
}

/// Refuses a body this verb cannot act on.
///
/// Separate from [`crate::handler::refuse`] because there is no service error
/// to carry: the request never reached the service. The code is the registry's
/// `UZ-REQ-001` either way, which is what keeps the two shapes answering alike.
fn reject(detail: &'static str) -> Response {
    let request_id = RequestId::mint();
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let code = afd_core::error_code::INVALID_REQUEST;
    let code_field = code.as_str();
    let request_id_field = request_id.as_str();
    tracing::debug!(
        error_code = code_field,
        request_id = request_id_field,
        // The DETAIL, never the body: an enrolment body carries a host
        // identifier and an operator's labels, and a refusal that quotes what
        // it refused is a refusal that logs it.
        detail,
        event = EVENT,
    );
    ProblemResponse::new(code, detail, request_id).into_response()
}
