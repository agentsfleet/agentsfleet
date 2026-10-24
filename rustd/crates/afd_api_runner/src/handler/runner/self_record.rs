//! `GET /v1/runners/me` — the runner's own row, read-only.
//!
//! Reading this does NOT bump liveness. Liveness is written by the heartbeat
//! and by nothing else, so inspecting a runner's row through this route
//! can never mask a dead runner (`docs/AUTH.md` §Runner token). That promise is
//! kept by the STATEMENT this reaches — `SELECT_RUNNER_SELF` has no update in
//! it — rather than by this handler remembering not to ask for one.

use std::borrow::Cow;
use std::sync::Arc;

use afd_runner::SelfRow;
use afd_runner::policy::capability;
use afd_wire::runner::SelfResponse;
use axum::Json;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::Services;

/// The scoped event a failed self read is logged under.
const EVENT: &str = "runner_self_read_failed";

/// Answers the runner's own registration row.
#[cfg_attr(feature = "openapi", utoipa::path(
    get,
    path = afd_wire::paths::RUNNER_SELF,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "get_runner_self",
    summary = "Read this runner's status",
    description = concat!(
        "Returns this runner's status, assigned policy, and supported operating-system ",
        "protections. Reading this endpoint does not mark the runner online; only ",
        "heartbeats update its connection status.",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = SelfResponse),
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
    match services.runners().self_record(runner.id()).await {
        // Assembled and serialised inside this arm, so every `Cow` borrowing
        // the row is written to the wire before the row is dropped — the
        // ownership split `SelfRow` documents, held by the borrow checker
        // rather than by statement order.
        Ok(row) => match payload(&row) {
            Ok(reply) => Json(reply).into_response(),
            Err(error) => refuse(&error, EVENT),
        },
        Err(error) => refuse(&error, EVENT),
    }
}

/// The row as the wire shape, borrowing every string from it.
///
/// `assigned_policy` resolves through the shared decoder, so a row this build
/// cannot read fails here exactly as it does on the heartbeat.
fn payload(row: &SelfRow) -> afd_runner::Result<SelfResponse<'_>> {
    Ok(SelfResponse {
        id: Cow::Borrowed(row.id.as_str()),
        status: Cow::Borrowed(&row.status),
        host_id: Cow::Borrowed(&row.host_id),
        sandbox_tier: Cow::Borrowed(&row.assignment.sandbox_tier),
        last_seen_at: row.last_seen_at,
        assigned_policy: row.assignment.decode()?,
        achievable: capability(row.capability_report_json.as_deref()),
        degraded: row.verdict.is_degraded(),
        degraded_reason: row.verdict.reason.as_deref().map(Cow::Borrowed),
    })
}
