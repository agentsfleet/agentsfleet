//! What a committed report still owes the world, none of it durable.
//!
//! Split from [`super`] at the commit: everything in `report.rs` and the
//! transaction it drives can still refuse the report, and nothing here can —
//! the money, the result, the session cursor and the freed slot have all
//! committed, so an error now would ask the runner to retry a report whose
//! money cannot be charged twice.
//!
//! Three things are left, and each is a fan-out rather than a fact: the frame
//! that tells a watching tail the run ended, the acknowledgement that takes the
//! entry off the fleet's stream, and the audit row that closes the lease's
//! history. Every one is attempted, logged if it does not land, and never
//! propagated.
//!
//! # The acknowledgement is last on purpose
//!
//! It is the one write here that loses work if it runs EARLY. Acknowledged
//! before the commit and then rolled back, the entry is gone from the stream
//! while `core.fleet_events` still says `received` — an event nothing will
//! ever deliver again. Acknowledged after, the worst case is an entry left
//! pending: it is redelivered, and the durable terminal state is what the
//! redelivery is measured against. So the ordering is not a preference, and
//! the queue's absence from [`super::Plane::report`]'s transaction is why it
//! cannot be anything else — no transaction spans Postgres and Dragonfly.

use afd_billing::Nanos;
use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::EventId;
use afd_events::Closed;
use afd_observability::producers;

use crate::error::Result;
use crate::lease::obligation::Owing;
use crate::lease::pull::Plane;
use crate::lease::settle::Reported;
use afd_outbound::obligation::Delivery;

/// A settled report was written.
const EVENT_SETTLED: &str = "report_settled";

/// A runner re-sent a report this lease had already settled.
const EVENT_ALREADY_SETTLED: &str = "report_already_settled";

/// The scoped event a post-commit step is logged under when it does not land.
const EVENT_STEP_FAILED: &str = "report_finalize_step_failed";

impl Plane {
    /// Announce a report that committed, and clear what it leaves behind.
    ///
    /// The order is the Zig's `finalize` minus the two writes that moved into
    /// the transaction: announce the ending, acknowledge the stream entry, then
    /// close the lease's own history.
    pub(super) async fn announce(
        &self,
        runner_id: &Uuid7,
        lease_id: &str,
        lease: &Reported,
        closed: Option<Box<Closed>>,
        charged: Nanos,
        now: UnixMillis,
    ) {
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

        if let Some(closed) = closed {
            self.leases.publish_completion(&closed).await;
        }
        self.acknowledge_entry(lease, lease_id).await;
        step(
            "record_released",
            lease,
            lease_id,
            self.leases
                .record_released(runner_id, lease_id, &lease.fleet_id, event, now)
                .await,
        );
    }

    /// Put an owed answer on the delivery queue, and receipt the row.
    ///
    /// Best-effort by construction, and logged rather than returned: the
    /// obligation is already committed, so a failure here leaves a row the
    /// producer sweep will re-append. Failing the report instead would tell a
    /// runner its finished, charged run did not land — and it did.
    pub(super) async fn queue_owed_answer(
        &self,
        owing: &Owing,
        lease: &Reported,
        answer: &str,
        now: UnixMillis,
    ) {
        step(
            "queue_delivery",
            lease,
            owing.obligation.as_str(),
            self.leases
                .queue_delivery(
                    &owing.obligation,
                    Delivery {
                        fleet_id: lease.fleet_id.as_str(),
                        workspace_id: lease.workspace_id.as_str(),
                        provider: owing.reply.provider,
                        destination: &owing.reply.address,
                        event_id: &lease.event_id,
                        answer,
                    },
                    now,
                )
                .await,
        );
    }

    /// Acknowledge the entry behind a report this lease had already settled.
    ///
    /// The runner is here because it never received the first response, and the
    /// likeliest reason the first response was lost is that the process died
    /// between the commit and the acknowledgement below — so the entry may well
    /// still be pending. Nothing else is re-run: the result, the cursor and the
    /// slot are durable from the first report, a second audit row would record
    /// a release that happened once as two, and re-announcing an ending would
    /// put a completion frame on the tail for a run that ended earlier.
    pub(super) async fn acknowledge_again(&self, lease: &Reported, lease_id: &str) {
        let fleet = lease.fleet_id.as_str();
        let event = lease.event_id.as_str();
        tracing::info!(
            fleet_id = fleet,
            agentsfleet_event_id = event,
            lease_id,
            event = EVENT_ALREADY_SETTLED,
            "the runner re-sent a settled report; its stored outcome stands"
        );
        self.acknowledge_entry(lease, lease_id).await;
    }

    /// Take the entry this lease executed off the fleet's stream.
    ///
    /// Addresses the RECEIPT, never the logical event id — see
    /// [`Leases::acknowledge`](crate::lease::Leases::acknowledge).
    async fn acknowledge_entry(&self, lease: &Reported, lease_id: &str) {
        step(
            "acknowledge",
            lease,
            lease_id,
            self.leases
                .acknowledge(&lease.fleet_id, &EventId::of(&lease.receipt))
                .await,
        );
    }
}

/// Log one post-commit step that did not land, and carry on.
///
/// Takes the already-awaited outcome rather than a future, so each call site
/// above reads as the step it names. The `step` label is what an operator greps
/// to find WHICH one is failing — a single `finalize_failed` line would say
/// only that something after the money did not work, which is the least
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
        event = EVENT_STEP_FAILED,
        "a post-commit report step did not land; the report still stands"
    );
}
