//! The run's memory, written by the report that carries it.
//!
//! ```text
//!   load (this runner's lease) ─► capture memory ─► commit the settle ─► publish
//!                                  └ fenced; a failure is logged, the report settles
//! ```
//!
//! Before the commit, because the commit frees the fleet's slot: a lease for
//! the fleet cannot issue until then, so the next lease's window reads what
//! this run learned. In its own transaction under the memory role, because the
//! settle's runs under the lease role and a lease store that could write
//! memory would need both. An idempotent upsert, so a report the fence then
//! refuses, or a retried one, writes the same rows again.
//!
//! A capture that fails does not refuse the report, the decision the runner's
//! own push made: withholding the answer would lose it too and let the lease
//! lapse into a re-run.

use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;
use afd_wire::report::ReportRequest;
use jiff::Timestamp;

use crate::lease::memory::Holder;
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;

/// The report's memory could not be written; the report still settles.
pub(super) const EVENT_CAPTURE_FAILED: &str = "memory_capture_failed";

/// The report's lease was no longer live, so its memory was not written again.
const EVENT_CAPTURE_SKIPPED: &str = "memory_capture_skipped";

impl Plane {
    /// Writes the report's memory deltas, fenced on the lease's own token.
    /// A fence refusal was logged by the fence; anything else is logged here.
    pub(super) async fn capture_reported(
        &self,
        runner_id: &Uuid7,
        lease: &Reported,
        request: &ReportRequest<'_>,
        now: Timestamp,
    ) {
        if request.memory.is_empty() {
            return;
        }
        // The row's token, as the settle is fenced on it: the report is this
        // runner's by the load, and a lease the fleet has moved past writes
        // neither its memory nor its settle.
        let holder = Holder {
            runner_id,
            fleet_id: &lease.fleet_id,
            lease_id: request.lease_id.as_ref(),
            fencing_token: lease.fence.as_u64(),
        };
        let Err(failure) = self.capture_for(holder, &request.memory, now).await else {
            return;
        };
        if failure.is_fence_refusal() {
            return;
        }
        // A lease no longer live: a repeated report, whose first post already
        // wrote this memory, or one the settle will refuse in a moment.
        if failure.code() == afd_core::error_code::RUN_LEASE_NOT_FOUND {
            let fleet_id = lease.fleet_id.as_str();
            let lease_id = request.lease_id.as_ref();
            let event = EVENT_CAPTURE_SKIPPED;
            tracing::debug!(fleet_id, lease_id, event);
            return;
        }
        let Logged { error_code, reason } = failure.logged();
        let fleet_id = lease.fleet_id.as_str();
        let lease_id = request.lease_id.as_ref();
        let event = EVENT_CAPTURE_FAILED;
        tracing::warn!(
            error_code,
            fleet_id,
            lease_id,
            reason,
            event,
            "the run's memory was not written; the report still settles"
        );
    }
}
