//! The claim a pass holds once it wins a fleet, how a pass ends, and the one
//! place the claim is let go.
//!
//! Every ending that issues no lease frees the claim — a refusal, a retry, a
//! park, a fault — and it is freed here, after the pass has its answer, so a
//! stop added later cannot forget to. An ending whose statement already freed
//! the slot says so, and costs nothing more.

use std::ops::ControlFlow;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_gate::gate::Waiting;
use afd_observability::metrics::label::fleet::IssueDenied;
use afd_observability::producers;
use jiff::Timestamp;

use super::Plane;
use super::Polled;
use crate::error::Result;
use crate::lease::affinity::{Backoff, Fence};
use crate::lease::answer::no_work;
use crate::lease::envelope::Acquired;

/// One step of the pass: carry on with `T`, or end with an [`Ending`].
pub(in crate::lease) type Step<T> = ControlFlow<Ending, T>;

/// How a pass that issued no lease ends, and what it still owes the slot.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::lease) enum Ending {
    /// The pass's own ending statement settled the admission and freed the
    /// slot: nothing more is owed. These are the bytes.
    Ended(String),
    /// Stop for a later poll; the slot is freed after `Backoff`. These are
    /// the bytes.
    Stop(String, Backoff),
    /// Stop and wait for a person; the park frees the slot. These are the
    /// bytes.
    Park(String),
}

/// A lease written and rendered: the bytes a runner reads.
///
/// The claim is the lease's now, so it is the one ending that keeps it.
pub(in crate::lease) struct Leased(pub(in crate::lease) String);

/// What a pass must let go of when it issues no lease.
struct Held {
    fleet_id: Uuid7,
    fence: Fence,
    event_id: String,
}

impl Plane {
    /// Every step after a won claim, then what becomes of the claim.
    ///
    /// A lease keeps it. Anything else frees it. A park stamps the admission
    /// and frees the slot together, so later work waits for the gate answer.
    ///
    /// `began` is when the poll began on the monotonic clock, so the issued
    /// lease's queue wait can count the time the poll itself took.
    pub(super) async fn run_claimed(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: Timestamp,
        began: Instant,
    ) -> Result<Polled> {
        let held = Held {
            fleet_id: acquired.fleet_id.clone(),
            fence: acquired.fence,
            event_id: acquired.event_id.clone(),
        };
        let pass = match self.admit_claimed(acquired, runner_id, now).await {
            Ok(ControlFlow::Continue(admitted)) => self.deliver(runner_id, admitted, now, began).await,
            Ok(ControlFlow::Break(ending)) => Ok(ControlFlow::Break(ending)),
            Err(fault) => Err(fault),
        };
        match pass {
            Ok(ControlFlow::Continue(Leased(answer))) => Ok(Polled::Lease(answer)),
            Ok(ControlFlow::Break(Ending::Ended(answer))) => Ok(Polled::Empty(answer)),
            Ok(ControlFlow::Break(Ending::Stop(answer, backoff))) => {
                self.let_go(&held, backoff).await;
                Ok(Polled::Empty(answer))
            }
            Ok(ControlFlow::Break(Ending::Park(answer))) => {
                if let Err(fault) = self
                    .leases
                    .park(&held.fleet_id, &held.event_id, held.fence)
                    .await
                {
                    self.let_go(&held, Backoff::Retry).await;
                    return Err(fault);
                }
                Ok(Polled::Empty(answer))
            }
            Err(fault) => {
                self.let_go(&held, Backoff::Retry).await;
                Err(fault)
            }
        }
    }

    /// Frees the claim through the one best-effort release every
    /// lease-less ending shares.
    async fn let_go(&self, held: &Held, backoff: Backoff) {
        self.leases.let_go(&held.fleet_id, held.fence, backoff).await;
    }
}

/// The single stable reason logged by both waiting-for-approval paths.
const AWAITING_APPROVAL: &str = "a human owes an answer";

/// A gate's wait, as an ending.
///
/// A question on the record parks: its answer, or its expiry, re-arms the
/// fleet. A wait that could not read whether a question exists stops behind
/// the retry backoff instead, and a later poll reads again — nothing would
/// re-arm a fleet on its behalf.
///
/// # Errors
/// Reports an answer that would not render.
pub(super) fn waited<T>(runner_id: &Uuid7, waiting: Waiting) -> Result<Step<T>> {
    let answer = no_work(runner_id, AWAITING_APPROVAL)?;
    Ok(ControlFlow::Break(match waiting {
        Waiting::Parked | Waiting::Pending => Ending::Park(answer),
        Waiting::Unreadable => Ending::Stop(answer, Backoff::Retry),
    }))
}

/// The pass outlived its claim, so the lease was not written.
const EVENT_CLAIM_LOST: &str = "lease_claim_lost";

/// The host refused the work it claimed, so the lease was not written.
const EVENT_ISSUE_DENIED: &str = "lease_issue_denied";

/// The no-work answer for a pass whose claim was superseded or lapsed before
/// its lease was written.
///
/// A stop with no backoff: the release is fenced on this pass's token, so it
/// cannot free the slot a new holder won, and a slot still under this fence
/// goes straight back for the next poll.
pub(in crate::lease) fn claim_lost<T>(runner_id: &Uuid7, acquired: &Acquired) -> Result<Step<T>> {
    // Hoisted: the `log` bridge duplicates field expressions and llvm-cov
    // scores the dead copy.
    let runner = runner_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_event_id = acquired.event_id.as_str();
    let fence = acquired.fence.as_i64();
    tracing::info!(
        event = EVENT_CLAIM_LOST,
        runner_id = runner,
        fleet_id,
        agentsfleet_event_id,
        fencing_token = fence,
        "the pass outlived its claim; no lease was written"
    );
    no_work(runner_id, CLAIM_LOST_REASON).map(|answer| ControlFlow::Break(Ending::Stop(answer, Backoff::None)))
}

/// The no-work answer for a claim the runner's host refused at issue.
///
/// The fleet itself is fine, so the slot goes back with no backoff and another
/// runner claims it on its next poll.
pub(in crate::lease) fn issue_denied<T>(
    runner_id: &Uuid7,
    acquired: &Acquired,
    reason: IssueDenied,
) -> Result<Step<T>> {
    producers::fleet::lease_issue_denied(reason);
    let runner = runner_id.as_str();
    let fleet_id = acquired.fleet_id.as_str();
    let agentsfleet_event_id = acquired.event_id.as_str();
    let why = reason.as_str();
    tracing::info!(
        event = EVENT_ISSUE_DENIED,
        runner_id = runner,
        fleet_id,
        agentsfleet_event_id,
        reason = why,
        "the runner's host refused the work it claimed; the fleet goes back"
    );
    no_work(runner_id, ISSUE_DENIED_REASON).map(|answer| ControlFlow::Break(Ending::Stop(answer, Backoff::None)))
}

/// The no-work reason a lost claim answers with.
const CLAIM_LOST_REASON: &str = "the claim lapsed before the lease was written";

/// The no-work reason a denied claim answers with.
const ISSUE_DENIED_REASON: &str = "the runner's host refused the claimed work";

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use std::ops::ControlFlow;

    use afd_core::id::{ENTROPY_LEN, Uuid7};
    use afd_gate::gate::Waiting;
    use jiff::Timestamp;

    use super::{Ending, Step, waited};
    use crate::lease::affinity::Backoff;

    fn runner() -> Uuid7 {
        Uuid7::encode(Timestamp::constant(1_767_225_600, 0), [3; ENTROPY_LEN])
            .expect("a fixed instant encodes")
    }

    /// A question on the record parks, because its answer or its expiry
    /// re-arms the fleet.
    #[test]
    fn a_wait_on_a_recorded_question_parks() {
        for waiting in [Waiting::Parked, Waiting::Pending] {
            let step: Step<()> = waited(&runner(), waiting).expect("the answer renders");
            assert!(
                matches!(step, ControlFlow::Break(Ending::Park(_))),
                "{waiting:?} must park"
            );
        }
    }

    /// A wait that could not read whether a question exists stops behind the
    /// retry backoff: nothing would re-arm the fleet on its behalf, and the
    /// oldest-first claim must not hand it straight back.
    #[test]
    fn an_unreadable_wait_stops_behind_the_backoff() {
        let step: Step<()> = waited(&runner(), Waiting::Unreadable).expect("the answer renders");
        assert!(matches!(
            step,
            ControlFlow::Break(Ending::Stop(_, Backoff::Retry))
        ));
    }

    /// A pass whose lease insert lost its fence answers no-work, as a stop
    /// with no backoff: the newer holder keeps its claim, and a slot still
    /// under this fence goes straight back.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_lost_claim_answers_no_work_as_a_stop() {
        use crate::lease::answer::no_work;
        use crate::lease::test_dead;

        let runner = runner();
        let step =
            super::claim_lost::<()>(&runner, &test_dead::acquired()).expect("the answer renders");
        let expected = no_work(&runner, super::CLAIM_LOST_REASON).expect("the answer renders");
        assert_eq!(
            step,
            ControlFlow::Break(Ending::Stop(expected, Backoff::None)),
            "a lost claim is the no-work answer, and never a lease"
        );
    }

    /// A host refusing its claim hands the fleet back with no backoff.
    #[cfg(feature = "test-util")]
    #[test]
    fn a_denied_claim_hands_the_fleet_back_at_once() {
        use afd_observability::metrics::label::fleet::IssueDenied;

        use crate::lease::answer::no_work;
        use crate::lease::test_dead;

        let runner = runner();
        let step = super::issue_denied::<()>(&runner, &test_dead::acquired(), IssueDenied::Capacity)
            .expect("the answer renders");
        let expected = no_work(&runner, super::ISSUE_DENIED_REASON).expect("the answer renders");
        assert_eq!(
            step,
            ControlFlow::Break(Ending::Stop(expected, Backoff::None))
        );
    }

    /// A fault after the claim reaches the caller as the fault, and the claim
    /// is still let go — here the release is refused too, so it is logged
    /// under the fault's own code and the claim lapses at its expiry instead.
    #[cfg(feature = "test-util")]
    #[tokio::test]
    async fn should_raise_the_fault_and_log_a_release_the_datastore_refuses() {
        use afd_core::test_util::trace::Capture;

        use crate::lease::affinity::EVENT_CLAIM_RELEASE_FAILED;
        use crate::lease::test_dead;

        let log = Capture::install();
        let acquired = test_dead::acquired();
        let fleet = acquired.fleet_id.clone();

        let fault = test_dead::plane()
            .run_claimed(
                acquired,
                &test_dead::id(9),
                test_dead::AT,
                std::time::Instant::now(),
            )
            .await
            .expect_err("an installed-fleet read with no datastore is a fault, not a decision");

        assert!(fault.is_datastore_unavailable(), "{fault}");
        let line = log.only(EVENT_CLAIM_RELEASE_FAILED).fields;
        assert_eq!(
            line.get("fleet_id").map(String::as_str),
            Some(fleet.as_str())
        );
        assert_eq!(
            line.get("error_code").map(String::as_str),
            Some(fault.code().as_str()),
            "the refused release carries the same outage's code"
        );
    }
}
