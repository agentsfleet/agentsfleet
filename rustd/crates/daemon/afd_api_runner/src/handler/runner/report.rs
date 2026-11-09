//! `POST /v1/runners/me/reports` — the terminal result of one run.
//!
//! Thin for the reason [`super::lease`] is: the fence, the flip, the money and
//! the five finalize writes are all `afd_fleet::lease::LeaseService`'s, and what is
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
//!
//! # Why the run's telemetry is described HERE and not where it settled
//!
//! A settled report is the one moment this daemon knows what a whole run cost
//! and how long it took, and two records are owed for it: a product event and
//! a delivery span. Neither is written on the money path. `afd_fleet` answers
//! with the facts — the charge, and the identity the lease row resolved — and
//! this handler decides what to say about them, so a lease plane stays usable
//! by a process with no exporter and no analytics project configured.

use std::sync::Arc;

use af_wire::report::{ReportRequest, ReportResponse};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use super::usage::{self, DETAIL_REPORT_MALFORMED};
use crate::auth::RunnerIdentity;
use crate::handler::{malformed, refuse};
use crate::services::{Leasing as _, Services};

/// The scoped event a failed report is logged under.
const EVENT: &str = "runner_report_failed";

/// What [`handle`] documents, its caps spelled from their constants.
#[cfg(feature = "openapi")]
const DESCRIPTION: &str = const_format::concatcp!(
    "Saves the final result of a work assignment. Replaced assignments are refused ",
    "without saving anything. Retry the same report safely using its lease ",
    "identifier; no `Idempotency-Key` is accepted. Repeats return the saved result ",
    "without another charge or run count. Invalid bodies return `UZ-REQ-001` with the ",
    "rejected field or limit: retired cumulative totals, more than ",
    af_wire::usage::LINES_MAX,
    " `requests`, or `seq` above ",
    af_wire::usage::RUNNER_SEQ_MAX,
    ".",
);

/// Records one terminal execution result.
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = af_wire::paths::RUNNER_REPORTS,
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_report",
    summary = "Report the result of one run",
    description = DESCRIPTION,
    request_body = ReportRequest<'_>,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = ReportResponse),
        (status = 400, description = afd_http::openapi::BAD_REQUEST),
        (status = 401, description = afd_http::openapi::UNAUTHORIZED),
        (status = 403, description = afd_http::openapi::FORBIDDEN),
        (status = 413, description = afd_http::openapi::PAYLOAD_TOO_LARGE),
        (status = 429, description = afd_http::openapi::TOO_MANY_REQUESTS),
        (status = 500, description = afd_http::openapi::INTERNAL),
        (status = 503, description = afd_http::openapi::UNAVAILABLE),
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
    let request = match afd_http::handler::read_body::<ReportRequest<'_>>(&body) {
        Ok(request) => request,
        // A runner from before usage moved to request lines is told which
        // field it sent; any other unreadable body gets the general sentence.
        Err(failure) => {
            return malformed(usage::retired_total(&failure).unwrap_or(DETAIL_REPORT_MALFORMED));
        }
    };
    // The request lines are money: one past the batch cap, or numbered in the
    // daemon's own range, is refused rather than partly priced, naming the
    // bound it broke.
    if let Err(report) = garde::Validate::validate(&request.requests) {
        return malformed(usage::REPORT_LINES.pick(&report));
    }

    let received = services.now();
    match services
        .leases()
        .report(runner.id(), &request, received)
        .await
    {
        Ok(settled) => {
            if let Err(error) = afd_fleet::lease::report::observe(services.analytics(), &settled, &request,
                runner.id().as_str(), received) {
                // Settlement succeeded; a metric conversion error must not make the runner replay a saved report.
                tracing::warn!(event = "report_observation_failed", error_code = error.code().as_str());
            }
            Json(ReportResponse { ok: true }).into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}

