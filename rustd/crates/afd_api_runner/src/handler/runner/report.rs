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
use std::time::{Duration, SystemTime};

use afd_fleet::lease::report::Reconciled;
use afd_observability::metrics::label::cost::{ChargeClass, ErrorType};
use afd_observability::producers::cost::{self, Spend};
use afd_observability::producers::fleet::runner;
use afd_observability::{Delivery, Telemetry};
use afd_wire::report::{Outcome, ReportRequest, ReportResponse};
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
            // Read here rather than reused from `services.now()`: that instant
            // was taken BEFORE the settle, and the span's end is the moment the
            // run's record actually closed. The Zig reads its clock at the same
            // point, for the same reason.
            delivery_of(&settled, &request).record(SystemTime::now());
            meter(&settled, &request, runner.id().as_str());
            Json(ReportResponse { ok: true }).into_response()
        }
        Err(error) => refuse(&error, EVENT),
    }
}

/// Records what the finished run cost, and what its runner did.
///
/// Everything here is derived from facts the settle already produced, so a
/// deployment exporting nothing pays one branch. The per-runner counters go
/// through the RAW identifier deliberately: the slot table decides what it is
/// attributed to, and a caller that chose the label would make the cardinality
/// ceiling a suggestion.
fn meter(settled: &Reconciled, request: &ReportRequest<'_>, runner_id: &str) {
    match request.failure_reason {
        // A failure the runner classified, and the classification is the label.
        reason @ Some(_) => runner::failed(runner_id, reason),
        // No class reported. `Outcome` still decides which it was: a fleet
        // error with no class is counted under the unmodelled bucket rather
        // than as a clean run, because the totals have to agree.
        None => match request.outcome {
            Outcome::Processed => runner::processed(runner_id),
            Outcome::FleetError => runner::failed(runner_id, None),
        },
    }

    cost::invocation(&Spend {
        model: &settled.model,
        posture: &settled.posture,
        input_tokens: u64::from(request.input_tokens),
        cached_input_tokens: u64::from(request.cached_input_tokens),
        output_tokens: u64::from(request.output_tokens),
        wall: Duration::from_millis(request.telemetry.wall_ms),
        error: (request.outcome == Outcome::FleetError).then_some(ErrorType::FleetError),
    });

    // The final slice only. Receive and renewal are charged on their own paths
    // and record their own class there — summing them here would double-count
    // every renewed run.
    if let Ok(nanocredits) = u64::try_from(settled.charged.as_i64()) {
        cost::credits_consumed(
            &settled.model,
            &settled.posture,
            ChargeClass::Settle,
            nanocredits,
        );
    }
}

/// The finished run, as the span that describes it.
///
/// Borrowed from both halves the settle produced: the lease row's identity,
/// which only the plane could resolve, and the runner's own counts, which only
/// the request carries.
///
/// Input tokens are SUMMED with the cached ones because the standard's
/// `gen_ai.usage.input_tokens` already includes the cached portion — cache
/// detail is its own subset metric, never a third additive direction.
fn delivery_of<'a>(settled: &'a Reconciled, request: &'a ReportRequest<'a>) -> Delivery<'a> {
    Delivery {
        tenant_id: settled.tenant_id.as_str(),
        workspace_id: settled.workspace_id.as_str(),
        fleet_id: settled.fleet_id.as_str(),
        event_id: &settled.event_id,
        posture: &settled.posture,
        provider: &settled.provider,
        model: &settled.model,
        input_tokens: u64::from(request.input_tokens) + u64::from(request.cached_input_tokens),
        output_tokens: u64::from(request.output_tokens),
        wall: Duration::from_millis(request.telemetry.wall_ms),
    }
}
