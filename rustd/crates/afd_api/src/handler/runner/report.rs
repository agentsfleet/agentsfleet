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
        Ok(_charged) => Json(ReportResponse { ok: true }).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}
