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

use afd_core::timing::HEARTBEAT_INTERVAL_MS;
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
#[cfg_attr(feature = "openapi", utoipa::path(
    post,
    path = "/v1/runners/me/heartbeats",
    tag = afd_http::openapi::tag::RUNNERS,
    operation_id = "runner_heartbeat",
    summary = "Report liveness, receive assignment",
    description = concat!(
        "Liveness up, configuration down. The assignment rides every reply. ",
        "An operator's dashboard change therefore reaches the host within one ",
        "interval, and nobody visits the host. An unreadable body is read as ",
        "`no report this beat` and the beat still counts. A host must not be ",
        "able to fail its own liveness by sending nonsense. ",
    ),
    request_body = Option<HeartbeatRequest>,
    responses(
        (status = 200, description = afd_http::openapi::OK, body = HeartbeatResponse),
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
    afd_http::handler::read_body(body).unwrap_or(NO_REPORT)
}

/// The cadence as the wire carries it.
///
/// [`HEARTBEAT_INTERVAL_MS`] is an `i64` because every span beside it is
/// compared against a `bigint` column; the wire quotes a duration, which is
/// never negative. The assertion proves the narrowing before the cast, so a
/// cadence edited past the wire's range fails the build rather than a beat.
const WIRE_INTERVAL_MS: u32 = {
    const _: () = assert!(
        HEARTBEAT_INTERVAL_MS > 0 && HEARTBEAT_INTERVAL_MS <= u32::MAX as i64,
        "the heartbeat cadence must be a positive duration the wire can carry"
    );
    HEARTBEAT_INTERVAL_MS as u32
};

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
        // The runner holds no cadence of its own: this daemon is what derives a
        // host offline, so it is what says how often to beat. `timing`'s
        // assertion keeps this under that threshold.
        heartbeat_interval_ms: WIRE_INTERVAL_MS,
    }
}

#[cfg(test)]
mod tests {
    use afd_core::timing::{HEARTBEAT_INTERVAL_MS, RUNNER_OFFLINE_AFTER_MS};

    use super::WIRE_INTERVAL_MS;

    /// What the runner is told must be the number this daemon enforces, not a
    /// second one that happens to agree today.
    #[test]
    fn test_the_served_cadence_is_the_enforced_cadence() {
        assert_eq!(i64::from(WIRE_INTERVAL_MS), HEARTBEAT_INTERVAL_MS);
    }

    /// The whole reason the cadence is served rather than held by the host: it
    /// has to stay under a threshold only this side knows.
    #[test]
    fn test_the_served_cadence_stays_below_the_offline_threshold() {
        assert!(
            i64::from(WIRE_INTERVAL_MS) < RUNNER_OFFLINE_AFTER_MS,
            "a host beating at {WIRE_INTERVAL_MS}ms would be derived offline at {RUNNER_OFFLINE_AFTER_MS}ms"
        );
    }
}
