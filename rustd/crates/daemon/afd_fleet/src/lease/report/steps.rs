//! Best-effort announcements after a report and its admission settle in Postgres.
//!
//! Tail frames, outbound appends, and lease audit events cannot undo acceptance
//! of the report. The committed outbound obligation repairs a missed append.

use af_core::id::Uuid7;
use af_observability::producers;
use af_wire::report::ReportRequest;
use afd_billing::Nanos;
use afd_events::Closed;
use jiff::Timestamp;

use crate::lease::fold::Folds;
use crate::lease::obligation::Owing;
use crate::lease::pull::LeaseService;
use crate::lease::settle::Reported;

/// The `event_complete` frame was published for a settled report.
pub(super) const EVENT_COMPLETION_PUBLISHED: &str = "completion_published";

/// A settled report was written.
const EVENT_SETTLED: &str = "report_settled";

/// A runner re-sent a report this lease had already settled.
const EVENT_ALREADY_SETTLED: &str = "report_already_settled";

/// Accepted completion payload carried across the commit boundary.
pub(super) struct Completion<'a> {
    pub closed: Option<Box<Closed>>,
    pub answer: &'a str,
}

impl LeaseService {
    /// Announce a report that committed, and clear what it leaves behind.
    ///
    /// The money and queue transitions already committed. Completion frames
    /// and the lease audit event are best effort after that boundary.
    pub(super) async fn announce(
        &self,
        runner_id: &Uuid7,
        request: &ReportRequest<'_>,
        lease: &Reported,
        completion: Completion<'_>,
        charged: Nanos,
    ) -> Option<Timestamp> {
        let Completion { closed, answer } = completion;
        let lease_id = request.lease_id.as_ref();
        // Hoisted for the `log` bridge's duplicated field expressions.
        let fleet = lease.fleet_id.as_str();
        let event = lease.event_id.as_str();
        let nanos = charged.as_i64();
        tracing::debug!(
            fleet_id = fleet,
            agentsfleet_event_id = event,
            lease_id,
            nanos,
            event = EVENT_SETTLED,
            "the report won its fence and its final slice was charged"
        );

        // After the fence is won, so a refused report does not decrement a
        // lease its runner still holds. The counterpart of the increment
        // `Leases::select` records when the lease was granted.
        producers::fleet::runner::lease_released(runner_id.as_str());

        // The completion publish is pinned here: after the two biller calls the
        // verb makes before this, because the follow-up a fleet owner sends on
        // seeing it is gated on the payer's balance through the biller, and
        // before the audit row, the owed answer and the token read below, which
        // nothing the owner sees depends on.
        // Timed only when the frame landed: a dropped announcement is logged
        // by the tail and leaves no completion instant to measure from.
        match closed {
            Some(closed) => match self.leases.publish_completion(&closed, Some(answer)).await {
                Ok(()) => {
                    let event = EVENT_COMPLETION_PUBLISHED;
                    tracing::debug!(fleet_id = fleet, lease_id, event);
                    Some(af_core::clock::now())
                }
                Err(error) => {
                    // The report is durable. Observe its failed live notification without undoing settlement.
                    afd_bus::observe_notification(
                        &afd_bus::Subject::FleetTail(closed.row.fleet_id.clone()),
                        Err(error),
                    );
                    None
                }
            },
            None => None,
        }
    }

    /// Ping the outbound workers that an owed answer committed.
    ///
    /// Best-effort by construction, and logged rather than returned: the
    /// obligation is already committed and claimable, so a lost ping costs
    /// one claim pass of latency. Failing the report instead would tell a
    /// runner its finished, charged run did not land — and it did.
    pub(super) async fn wake_owed_answer(&self, owing: &Owing) {
        self.leases.wake_delivery(&owing.obligation).await;
    }

    /// Record and announce what the run's folds settled.
    ///
    /// Best effort, as the run's own announcement is: every row is committed,
    /// so a frame that does not land is re-read by the next page. The settled
    /// log and counter are written here, after the commit, so a report that
    /// rolled back leaves neither.
    pub(super) async fn announce_folds(
        &self,
        lease: &Reported,
        lease_id: &str,
        folds: Folds,
        answer: &str,
    ) {
        folds.record_settled(lease.fleet_id.as_str(), lease_id);
        for closed in &folds.closed {
            let publication = self.leases.publish_completion(closed, Some(answer)).await;
            afd_bus::observe_notification(
                &afd_bus::Subject::FleetTail(closed.row.fleet_id.clone()),
                publication,
            );
        }
    }

    /// A repeated report returns the stored outcome without duplicating side effects.
    pub(super) fn record_repeat(lease: &Reported, lease_id: &str) {
        let fleet = lease.fleet_id.as_str();
        let event = lease.event_id.as_str();
        tracing::info!(
            fleet_id = fleet,
            agentsfleet_event_id = event,
            lease_id,
            event = EVENT_ALREADY_SETTLED,
            "the runner re-sent a settled report; its stored outcome stands"
        );
    }
}
