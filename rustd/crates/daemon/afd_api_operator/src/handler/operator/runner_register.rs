//! Registering a runner: a platform runner at `POST /v1/admin/runners`, and a
//! tenant's own at `POST /v1/tenants/me/runners`.
//!
//! # The token is revealed once
//!
//! The row stores a digest. The value exists only while this response is
//! written, `Minted` zeroes it on drop, and nothing here logs the payload.

use std::borrow::Cow;
use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::runner::{AssignedPolicy, RegisterRequest, RegisterResponse, RegistrationReplay};
use afd_http::handler::Refusal;
use afd_runner::{Enrolled, Registered};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};
use http::{HeaderMap, StatusCode, header};

use crate::handler::refuse;
use crate::services::Services;

/// The scoped event a failed registration is logged under.
const EVENT: &str = "runner_registration_failed";

/// The refusal for an absent body.
const DETAIL_BODY_REQUIRED: &str = "Request body required";

/// The refusal for a body that could not be read, naming the shape. Pinned
/// byte for byte: a person registering a host by hand reads it to learn what
/// they got wrong.
const DETAIL_MALFORMED_BODY: &str = "Malformed JSON body (host_id, assigned_policy{sandbox_tier, \
                                     network_policy, registry_allowlist[], worker_count}, labels[])";

/// Registers a platform runner, answering its identity and one-time credential.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::ADMIN_RUNNERS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "register_platform_runner",
    summary = "Register a platform runner",
    description = concat!(
        "Registers a platform runner and assigns its policy. Requires `runner:register` on a ",
        "bearer credential. Save the returned `agt_r` token securely; it appears once. The ",
        "runner must apply the assigned policy.",
    ),
    request_body = RegisterRequest<'_>,
    responses(
        (status = 201, description = "Runner registered; the bearer token is returned exactly once", body = RegisterResponse<'_>),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        afd_http::openapi::GuardedFailures,
    ),
))]
pub(crate) async fn platform<D: Services>(State(services): State<Arc<D>>, body: Bytes) -> Response {
    let request = match read(&body) {
        Ok(request) => request,
        Err(refused) => return refused.into_response(),
    };
    match services.runners().register(&request, services.now()).await {
        Ok(registered) => created(&registered, &request.assigned_policy),
        Err(error) => refuse(&error, EVENT),
    }
}

/// Registers one of `tenant`'s runners under the request's Idempotency-Key.
///
/// The stored policy has durable tasks off, whatever the body asked for: only a
/// platform admin switches them on for a tenant runner. A replay answers the
/// runner the key already registered, without its token.
pub(super) async fn tenant<D: Services>(
    services: &D,
    tenant: &Uuid7,
    headers: &HeaderMap,
    body: &Bytes,
) -> Response {
    let operation = match afd_http::handler::setup_input::operation(headers) {
        Ok(operation) => operation,
        Err(refused) => return refused.into_response(),
    };
    let request = match read(body) {
        Ok(request) => request,
        Err(refused) => return refused.into_response(),
    };
    let stored = AssignedPolicy {
        durable_tasks_enabled: false,
        ..request.assigned_policy.clone()
    };
    match services
        .runners()
        .register_tenant(tenant, &operation, &request, services.now())
        .await
    {
        Ok(Registered::Created(registered)) => created(&registered, &stored),
        Ok(Registered::Replayed {
            runner_id,
            worker_count,
        }) => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "no-store")],
            Json(RegistrationReplay {
                runner_id: Cow::Owned(runner_id.to_string()),
                assigned_policy: AssignedPolicy {
                    worker_count,
                    ..stored
                },
            }),
        )
            .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The request, read strictly: a misspelled key at any depth is refused rather
/// than silently assigning a policy nobody asked for.
fn read(body: &Bytes) -> Result<RegisterRequest<'_>, Refusal> {
    if body.is_empty() {
        return Err(Refusal::malformed(DETAIL_BODY_REQUIRED));
    }
    // The parser's failure names nothing a caller can act on, and the unknown
    // field it refused is already logged by `read_strict_body`.
    let Ok(request) = afd_http::handler::read_strict_body::<RegisterRequest<'_>>(body) else {
        return Err(Refusal::malformed(DETAIL_MALFORMED_BODY));
    };
    Ok(request)
}

/// The `201`, carrying the credential this response exists to reveal, with the
/// assignment as stored so the caller reads what the host will apply.
fn created(registered: &Enrolled, requested: &AssignedPolicy<'_>) -> Response {
    let stored = AssignedPolicy {
        worker_count: registered.worker_count,
        ..requested.clone()
    };
    (
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(RegisterResponse {
            runner_id: Cow::Borrowed(registered.runner_id.as_str()),
            runner_token: Cow::Borrowed(registered.token.expose()),
            assigned_policy: stored,
        }),
    )
        .into_response()
}
