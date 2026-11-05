//! An ungranted delivery: the grant it needs is asked for, and the answer
//! decides whether it parks, retries or ends.
//!
//! Split from [`super`] at the file cap, on the seam between building a
//! policy and asking a person for the access one needs.

use afd_approval::{Origin, Requested, Wanted};
use afd_core::error_code::{Coded as _, Logged};
use afd_core::event::label;
use afd_core::id::Uuid7;
use jiff::Timestamp;
use std::ops::ControlFlow;

use crate::error::Result;
use crate::lease::affinity::Backoff;
use crate::lease::answer::no_work;
use crate::lease::envelope::Acquired;
use crate::lease::pull::{Ending, Leased, Plane};

impl Plane {
    /// Ask for the grant this delivery needs, and say what the poll answers.
    ///
    /// The backstop, not the design: a fleet installed after M194 leaves
    /// install with this card already raised, and reaching here means the fleet
    /// predates that or the install-time request could not be written. Asking
    /// again is what makes the invariant unconditional — a park is a question,
    /// never a silent loop — and the request is idempotent, so the one-second
    /// redelivery cadence raises one card rather than sixty a minute.
    ///
    /// A DENIED grant ends the event. The gate that denied it carries no
    /// `event_id` and so could not end anything itself; this is where a
    /// person's no stops the redelivery. An open card parks the delivery, and
    /// anything else stops it for the next poll to ask again.
    pub(super) async fn ungranted(
        &self,
        runner_id: &Uuid7,
        acquired: &Acquired,
        credential: &str,
        integration: &str,
        now: Timestamp,
    ) -> Result<ControlFlow<Ending, Leased>> {
        let asked = self
            .grants
            .request(
                &acquired.workspace_id,
                &acquired.fleet_id,
                Wanted {
                    service: integration,
                    credential,
                    origin: Origin::Park,
                },
                now,
            )
            .await;
        let reason = format!("{credential} needs a grant for {integration}");
        match answers(written(asked, &acquired.fleet_id, integration)) {
            Ungranted::Ends => self
                .refused(acquired, label::GRANT_DENIED, runner_id, &reason)
                .await
                .map(|answer| ControlFlow::Break(Ending::Ended(answer))),
            waits => {
                no_work(runner_id, &reason).map(|answer| ControlFlow::Break(waits.ending(answer)))
            }
        }
    }
}

/// A park could not raise its card, and the write failed rather than the human.
pub(super) const EVENT_REQUEST_FAILED: &str = "park_grant_request_failed";

/// The request's answer, with a failure to write one REPORTED before it is dropped.
///
/// The error dies here either way — [`answers`] never ends a delivery on an
/// unwritten request, because a datastore that would not answer is this
/// instance's problem and reading its silence as a refusal would end deliveries
/// on an outage. What it must not do is die QUIETLY. A park that could not raise
/// its card answers the same `no_work` with the same reason as a park waiting on
/// a person, so without this line the two are indistinguishable in the log while
/// the delivery redelivers every second — which is precisely the failure with no
/// error that this milestone exists to end, reproduced on the path that ends it.
///
/// [`crate::lease::pull::Plane::refused`] already reports the denial and
/// `IntegrationGrants::request` reports what it wrote; the error was the one
/// outcome nothing spoke for. The install path says the same thing at its own
/// call site (`afd_fleet_lifecycle::install::grants`).
pub(super) fn written(
    asked: Result<Requested, afd_approval::Error>,
    fleet: &Uuid7,
    service: &str,
) -> Option<Requested> {
    match asked {
        Ok(answer) => Some(answer),
        Err(unwritten) => {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the dead copy.
            let Logged { error_code, reason } = unwritten.logged();
            let fleet_id = fleet.as_str();
            tracing::warn!(
                error_code,
                event = EVENT_REQUEST_FAILED,
                fleet_id,
                service,
                reason,
                "the delivery parked without raising its grant card; the next poll asks again"
            );
            None
        }
    }
}

/// What an ungranted delivery does once the grant has been asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Ungranted {
    /// A card is open and a person owes the answer, which re-marks the fleet:
    /// the delivery parks until then.
    Parks,
    /// The grant just landed since the assembly read: nobody owes an
    /// answer, so the delivery is leasable again at once.
    Retries,
    /// The card could not be WRITTEN, so nobody can answer it: the delivery
    /// stays leasable behind the retry backoff, and a later poll asks again.
    Unwritten,
    /// The event ends: nothing about the next poll would be different.
    Ends,
}

impl Ungranted {
    /// The ending an ungranted delivery takes once its answer is rendered: an
    /// open card parks it, and anything else stops it — for the next poll to
    /// ask again, or, for [`Ungranted::Ends`], after the refusal was written.
    pub(super) fn ending(self, answer: String) -> Ending {
        match self {
            Self::Parks => Ending::Park(answer),
            Self::Retries => Ending::Stop(answer, Backoff::None),
            Self::Unwritten => Ending::Stop(answer, Backoff::Retry),
            Self::Ends => Ending::Ended(answer),
        }
    }
}

/// Whether an ungranted delivery parks, retries, or ends on an answer
/// already given.
///
/// A person's NO is the only outcome that makes the next poll pointless — every
/// other reading leaves a question a human can still answer, and ending an
/// event on any of them would throw away work nobody refused.
///
/// Only an OPEN card parks, because parking clears the fleet's mark and only
/// an answer or an expiry puts it back. A grant approved since the assembly
/// read has no answer left to wait for, and `None` — a request that could not
/// be WRITTEN — has no card anyone could answer; either would wait forever on
/// a park, so both retry on the next poll. Fail-closed here means keeping the
/// event alive: a datastore that would not answer is this instance's problem,
/// and reading its silence as a refusal would end deliveries on an outage.
pub(super) const fn answers(asked: Option<Requested>) -> Ungranted {
    match asked {
        Some(Requested::Denied) => Ungranted::Ends,
        Some(Requested::Raised | Requested::Pending) => Ungranted::Parks,
        Some(Requested::Approved) => Ungranted::Retries,
        None => Ungranted::Unwritten,
    }
}
