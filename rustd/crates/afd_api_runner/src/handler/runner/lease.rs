//! `POST /v1/runners/me/leases` — the poll a runner lives on.
//!
//! Thinner than it looks, and deliberately: the whole decision — claim, money,
//! gates, policy, row — is `afd_fleet::lease::Plane`'s, and it answers with
//! the bytes it decided on. What is left here is the two things that are
//! genuinely this layer's: which identity is asking, and what a failure looks
//! like on the wire.
//!
//! # The body is not read
//!
//! The runner posts `{}` and this port does not look at it. One shape is served
//! unconditionally, with no negotiation, no downgrade and no "unsupported
//! version" refusal. The request has no extractor for its body at all, which is
//! the strongest way to say the body changes nothing: there is no code path a
//! future edit could make read it by accident.
//!
//! There is no `LeaseRequest` type either, and that is deliberate. It existed
//! to carry a `wire_version` from M157 until Sep 2026 that nothing ever read;
//! with the field gone the type described an empty body nobody parsed, was
//! absent from `public/openapi.json` because this route declares no
//! `requestBody`, and had no consumer left but tests asserting its own shape.
//! A wire type the daemon does not deserialize and the document does not
//! publish is decoration, so it was deleted rather than kept building.
//!
//! # Always 200, and never 204
//!
//! Work and no-work are the same status and the same shape —
//! `{"lease":…,"retry_after_ms":…}`. A 204 would make "nothing to do" a
//! different response class from "here is something to do", and a runner would
//! need two parsers for one poll.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed poll is logged under.
const EVENT: &str = "runner_lease_failed";

/// The content type the answer is already serialized as.
const APPLICATION_JSON: HeaderValue = HeaderValue::from_static("application/json");

/// Hands the runner its next lease, or a backoff.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_lease",
    summary = "Poll for the next lease",
    description = concat!(
        "The poll a runner lives on. Answers either a lease to run or a ",
        "backoff to wait out. The claim, the money, the gates and the policy ",
        "are all decided before the answer is written. ",
    ),
    responses(
        (status = 200, description = "A lease to run, or a backoff to wait out", body = afd_wire::lease::LeaseResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
) -> Response {
    match services
        .leases()
        .lease(runner.id(), runner.is_degraded(), services.now())
        .await
    {
        // Already JSON, because the policy inside it borrows from values that
        // do not outlive the call that built them — see `lease::pull`. Handing
        // back bytes rather than a `Json<T>` is what keeps a single assembly.
        Ok(body) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, APPLICATION_JSON)],
            body,
        )
            .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
