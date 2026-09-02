//! `POST /v1/runners/me/leases/{lease_id}/renew` — a live run buying more time.
//!
//! # The body is optional and its failure is not
//!
//! `RenewRequest` carries cumulative token counts, and a runner that has not
//! wired token accounting yet sends nothing at all. An absent or unreadable
//! body therefore meters at zero tokens — run fee only — rather than refusing:
//! the alternative kills a healthy child over a serialization disagreement,
//! and the runtime is genuinely owed either way. `UZ-RUN-013` exists in the
//! registry for this and is deliberately never ANSWERED, only logged, which is
//! the Zig's behaviour too.
//!
//! # Why the lease id comes off the path
//!
//! One lease per request, named where a reader of the access log can see it.
//! The ownership check is not here — the load is scoped by runner id inside the
//! plane, so a runner naming a peer's lease gets the same 404 a missing one
//! gets, and this layer never has to know that is what happened.

use std::sync::Arc;

use afd_wire::report::{RenewRequest, RenewResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed renewal is logged under.
const EVENT: &str = "runner_renew_failed";

/// A body arrived and could not be read; the slice meters runtime only.
const EVENT_BODY_INVALID: &str = "renew_body_parse_failed";

/// Extends one live lease's deadline.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/leases/{lease_id}/renew",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_renew_lease",
    summary = "Extend a live lease",
    description = concat!(
        "A run buying more time before its lease expires. The body is ",
        "optional. A body that will not parse is refused rather than read ",
        "as an empty one. A renewal asserts progress, and an unreadable ",
        "assertion is not one. ",
    ),
    params(
        afd_http::openapi::path::Lease,
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = RenewResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    Path(lease_id): Path<String>,
    body: Bytes,
) -> Response {
    match services
        .leases()
        .renew(runner.id(), &lease_id, counts(&body), services.now())
        .await
    {
        Ok(expires_at) => Json(RenewResponse {
            lease_expires_at: expires_at.as_millis(),
        })
        .into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The cumulative counts the body carried, or none.
///
/// Default-safe by design — see this module's note. The empty case is silent
/// because it is the steady state for today's runner; a body that arrived and
/// would not parse is logged, because that one means two builds disagree about
/// a shape and somebody should know.
fn counts(body: &[u8]) -> RenewRequest {
    if body.is_empty() {
        return RenewRequest::default();
    }
    afd_core::json::object_from_slice(body).unwrap_or_else(|_unreadable| {
        tracing::warn!(
            event = EVENT_BODY_INVALID,
            "the renew body could not be read; this slice meters runtime only"
        );
        RenewRequest::default()
    })
}
