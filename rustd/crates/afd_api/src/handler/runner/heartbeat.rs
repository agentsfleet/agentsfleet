//! `POST /v1/runners/me/heartbeats` — liveness up, assignment down.
//!
//! # The reply is unconditionally `ok`
//!
//! Rejection is authentication's job. A runner whose row is cordoned, drained
//! or revoked never reaches this handler — the layer in front of it refuses
//! with `UZ-RUN-009` — so a `drain` or `stop` status arriving from here would
//! be a second, weaker channel for a decision that already has one. The
//! fleet-failover slice is what will populate the other two statuses; until
//! then a beat that got this far is a beat that keeps working.
//!
//! # A malformed body is not a failed beat
//!
//! `parseCapabilityReport` reads an unparseable body as "no report this beat"
//! and beats anyway, and that is the behaviour a runner depends on: a token
//! must not be able to fail its own liveness by sending nonsense, because a
//! host that cannot beat is a host the fleet reads as dead. So the body is
//! parsed LENIENTLY here — anything unreadable becomes
//! [`afd_runner::NO_REPORT`] — and the bounds on what does parse are the
//! service's (`afd_runner::bounds`).
//!
//! That leniency stops at the size limit, which is enforced before this runs:
//! `hyper` refuses an oversize head and axum's body limit refuses an oversize
//! body, so an amplification attempt never reaches the parser at all.

use std::borrow::Cow;
use std::sync::Arc;

use afd_runner::{Beat, NO_REPORT};
use afd_wire::runner::{HeartbeatRequest, HeartbeatResponse, HeartbeatStatus};
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::response::{IntoResponse as _, Response};

use crate::auth::RunnerIdentity;
use crate::handler::refuse;
use crate::services::Services;

/// The scoped event a failed beat is logged under.
const EVENT: &str = "runner_heartbeat_failed";

/// Records a beat and answers what the host must apply.
pub(crate) async fn handle<D: Services>(
    State(services): State<Arc<D>>,
    RunnerIdentity(runner): RunnerIdentity,
    body: Bytes,
) -> Response {
    // Borrowed out of `body`, which outlives the call: the capability report a
    // host sends on its first beat is re-serialised into the row, and copying
    // it twice on the path every host takes every ten seconds is the kind of
    // cost that only shows up at fleet scale.
    let beat = read(&body);
    match services
        .runners()
        .heartbeat(runner.id(), &beat, services.now())
        .await
    {
        Ok(beat) => Json(payload(&beat)).into_response(),
        Err(error) => refuse(&error, EVENT),
    }
}

/// What the body carried, or nothing at all.
///
/// The one place the leniency in this module's documentation is spent. It is
/// deliberately silent: a host sending a body this daemon cannot read is a host
/// running a build that disagrees about the shape, which is worth a metric
/// eventually and is worth nothing in a log line per beat per host.
fn read(body: &[u8]) -> HeartbeatRequest<'_> {
    if body.is_empty() {
        return NO_REPORT;
    }
    afd_core::json::object_from_slice(body).unwrap_or(NO_REPORT)
}

/// The beat as the wire shape, borrowing the assignment from the row.
fn payload(beat: &Beat) -> HeartbeatResponse<'_> {
    HeartbeatResponse {
        status: HeartbeatStatus::Ok,
        // Carried on EVERY beat, so an operator's dashboard change reaches the
        // host within one interval and nobody visits the host. A null
        // assignment means a row this daemon could not read, and the runner
        // fails closed on it rather than leasing under a policy it invented.
        assigned_policy: beat.assignment.decode(),
        degraded: beat.verdict.is_degraded(),
        degraded_reason: beat.verdict.reason().map(Cow::Borrowed),
        selftest_requested: beat.selftest_requested,
    }
}
