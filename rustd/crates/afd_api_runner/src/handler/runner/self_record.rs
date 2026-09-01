//! `GET /v1/runners/me` — the runner's own row, read-only.
//!
//! Reading this does NOT bump liveness. Liveness is written by the heartbeat
//! and by nothing else, so inspecting a host with `agentsfleet-runner status`
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
    path = "/v1/runners/me",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "get_runner_self",
    summary = "Read this runner's own row",
    description = concat!(
        "Answers the row this host enrolled as: its status, its assigned ",
        "policy, and what its kernel can actually enforce. Reading it does ",
        "NOT bump liveness — the heartbeat writes that and nothing else does, ",
        "so inspecting a host can never mask a dead runner. ",
    ),
    responses(
        (status = 200, description = afd_http::openapi::OK, body = SelfResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
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
        // rather than by the `defer q.deinit()` ordering the Zig relies on.
        Ok(row) => Json(payload(&row)).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// The row as the wire shape, borrowing every string from it.
///
/// `assigned_policy` and `achievable` resolve through the shared decoder, so a
/// missing or unparseable column reads as absent here exactly as it does on the
/// heartbeat — and the host fails closed on either.
fn payload(row: &SelfRow) -> SelfResponse<'_> {
    SelfResponse {
        id: Cow::Borrowed(row.id.as_str()),
        // The column is `admin_state`; the runner-facing field has always been
        // `status`. The rename stopped at the schema, and this is where that
        // stops being visible to a host.
        status: Cow::Borrowed(&row.status),
        host_id: Cow::Borrowed(&row.host_id),
        sandbox_tier: Cow::Borrowed(&row.assignment.sandbox_tier),
        last_seen_at: row.last_seen_at,
        assigned_policy: row.assignment.decode(),
        achievable: capability(row.capability_report_json.as_deref()),
        degraded: row.verdict.degraded,
        degraded_reason: row.verdict.reason.as_deref().map(Cow::Borrowed),
    }
}
