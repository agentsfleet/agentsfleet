//! `POST /v1/runners/me/reports` — the terminal result of one run.
//!
//! Thin for the reason [`super::lease`] is: the fence, the flip, the money and
//! the five finalize writes are all `afd_fleet::lease::Plane`'s, and what is
//! left here is which identity is asking and what a refusal looks like on the
//! wire.
//!
//! # The body IS read, and strictly
//!
//! Unlike the lease poll, this request carries the only copy of what happened —
//! the verdict, the token counts a tenant is charged for, and the cursor the
//! session resumes from. `ReportRequest` is `deny_unknown_fields`, so a runner
//! from a newer build sending a field this daemon does not know is refused
//! rather than silently charged against a partial reading.
//!
//! # Every refusal here is terminal for the run
//!
//! A stale fence, a lease that is not this runner's — both mean the result is
//! not wanted and retrying cannot change that. The runner discards and moves
//! on. Only a datastore failure is worth a retry, and that arrives as a 503
//! through the transport class the code carries, which is what the runner
//! already backs off on.

use std::sync::Arc;

use afd_observability::Telemetry;
use afd_wire::report::{ReportRequest, ReportResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::{Leasing as _, Services};

/// The scoped event a failed report is logged under.
const EVENT: &str = "runner_report_failed";

/// The refusal a body this daemon cannot read earns.
const DETAIL_MALFORMED: &str = "Malformed report body";

/// Records one terminal execution result.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/reports",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_report",
    summary = "Report the result of one run",
    description = concat!(
        "The terminal result of one lease. A report from a holder the fleet ",
        "has already superseded is refused by the fence and writes nothing. ",
        "A stale writer therefore cannot land a partial finalize on the ",
        "current holder's run. ",
    ),
    request_body = ReportRequest,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ReportResponse),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 500, description = afd_http::openapi::INTERNAL),
    ),
))]
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Response {
    // Borrowed out of `body`, which outlives the call. The response text and
    // the checkpoint are the two largest values on this path and both are
    // written straight into columns, so owning them here would copy a run's
    // whole output for nothing.
    let Ok(request) = afd_core::json::object_from_slice::<ReportRequest<'_>>(&body) else {
        return crate::envelope::ProblemResponse::new(
            afd_core::error_code::INVALID_REQUEST,
            DETAIL_MALFORMED,
            crate::request_id::RequestId::mint(),
        )
        .into_response();
    };

    match services
        .leases()
        .report(runner.id(), &request, services.now())
        .await
    {
        // The drained amount is answered by the plane and deliberately not put
        // on the wire: a runner has no use for it, and the reply the stock
        // runner parses is `{"ok":true}`. It exists so M181 §5 can attach the
        // credit instrument without reopening the money path.
        Ok(settled) => {
            // Attributed to the WORKSPACE, as the daemon this ports attributes
            // it: a run has no person behind it — a cron, a webhook and a steer
            // all end here — and the workspace is the unit the funnels count.
            services.analytics().report(&Telemetry::FleetCompleted {
                actor: settled.workspace_id.as_str().to_owned(),
                workspace_id: settled.workspace_id.as_str().to_owned(),
                fleet_id: settled.fleet_id.as_str().to_owned(),
                event_id: request.event_id.as_ref().to_owned(),
                tokens: request.tokens,
                wall_ms: request.telemetry.wall_ms,
                exit_status: request.outcome.as_str().to_owned(),
                time_to_first_token_ms: u64::from(request.telemetry.time_to_first_token_ms),
            });
            Json(ReportResponse { ok: true }).into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}
