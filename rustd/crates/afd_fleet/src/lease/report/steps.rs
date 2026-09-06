//! The five durable writes a won report still owes, none of them fatal.
//!
//! Split from [`super`] at the commit: everything in `report.rs` can still
//! refuse the report, and nothing here can — the claim and the settle have
//! committed, so an error now would ask the runner to retry a report whose
//! money cannot be charged twice, and the retry would be fenced anyway.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::report::ReportRequest;

use crate::error::Result;
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;
use crate::lease::verdict::{Terminal, Verdict};

/// The scoped event a finalize step is logged under when it does not land.
const EVENT_FINALIZE_FAILED: &str = "report_finalize_step_failed";

impl Plane {
    /// The five durable writes a won report still owes, none of them fatal.
    ///
    /// Every step is attempted independently and logged if it does not land.
    /// Nothing here can fail the verb: the claim and the settle have already
    /// committed, so an error now would ask the runner to retry a report whose
    /// money cannot be charged twice — and the retry would be fenced anyway,
    /// turning a partial success into a permanent 409.
    ///
    /// The order is the Zig's `finalize`: end the work, checkpoint where it
    /// resumes, acknowledge the stream entry, free the slot, then close the
    /// lease's own history. The slot release is late deliberately — it makes
    /// the fleet's next event claimable, and doing that before the event row is
    /// terminal would let a fresh lease race a half-written finalize.
    pub(super) async fn finalize(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        lease: &Reported,
        request: &ReportRequest<'_>,
        verdict: Verdict<'_>,
        now: UnixMillis,
    ) {
        let event_id = lease.event_id.as_str();
        let wall_ms = i64::try_from(request.telemetry.wall_ms).unwrap_or(i64::MAX);
        let tokens = i64::try_from(request.tokens).unwrap_or(i64::MAX);

        let outcome = Terminal {
            verdict,
            response_text: request.response_text.as_ref(),
            tokens,
            wall_ms,
        };
        self.close_row(lease, lease_id, outcome, now).await;
        step(
            "checkpoint",
            lease,
            lease_id,
            self.leases
                .checkpoint(
                    &lease.fleet_id,
                    request.checkpoint.last_event_id.as_ref(),
                    request.checkpoint.last_response.as_ref(),
                    now,
                )
                .await,
        );
        step(
            "acknowledge",
            lease,
            lease_id,
            self.leases.acknowledge(&lease.fleet_id, event_id).await,
        );
        step(
            "release_slot",
            lease,
            lease_id,
            self.leases
                .release_slot(&lease.fleet_id, lease.fence, now)
                .await,
        );
        step(
            "record_released",
            lease,
            lease_id,
            self.leases
                .record_released(runner_id, lease_id, &lease.fleet_id, event_id, now)
                .await,
        );
    }
}

impl Plane {
    /// End the event row, and announce the ending the write returned.
    ///
    /// The closing bracket rides the row the terminal write hands back, so
    /// the tail learns the ending in the same round trip that wrote it. A row
    /// that was already terminal — a redelivery whose acknowledgement was
    /// lost — has no new ending to announce, and a write that did not land is
    /// logged the way every other finalize step is.
    async fn close_row(
        &self,
        lease: &Reported,
        lease_id: &str,
        outcome: Terminal<'_>,
        now: UnixMillis,
    ) {
        match self
            .leases
            .mark_terminal(&lease.fleet_id, &lease.event_id, outcome, now)
            .await
        {
            Ok(Some(closed)) => self.leases.publish_completion(&closed).await,
            Ok(None) => {}
            Err(failure) => step("mark_terminal", lease, lease_id, Err(failure)),
        }
    }
}

/// Log one finalize step that did not land, and carry on.
///
/// Takes the already-awaited outcome rather than a future, so each call site
/// above reads as the step it names. The `step` label is what an operator greps
/// to find WHICH of the five is failing — a single `finalize_failed` line would
/// say only that something after the money did not work, which is the least
/// actionable version of this warning.
pub(super) fn step(step: &'static str, lease: &Reported, lease_id: &str, outcome: Result<()>) {
    let Err(failure) = outcome else {
        return;
    };
    let fleet = lease.fleet_id.as_str();
    let event = lease.event_id.as_str();
    let reason = failure.to_string();
    let code = failure.code().as_str();
    tracing::warn!(
        error_code = code,
        fleet_id = fleet,
        agentsfleet_event_id = event,
        lease_id,
        step,
        reason,
        event = EVENT_FINALIZE_FAILED,
        "a post-settle finalize step did not land; the report still stands"
    );
}
