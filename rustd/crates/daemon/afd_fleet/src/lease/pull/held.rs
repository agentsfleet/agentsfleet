//! The claim a pass holds once it wins a fleet, and the one place it is let
//! go.
//!
//! Every ending that issues no lease frees the claim — a refusal, a retry, a
//! park, a fault — and it is freed here, after the pass has its answer, so a
//! stop added later cannot forget to. Each of those endings used to return
//! with the claim still held, and the fleet's next event waited out the
//! claim's whole lifetime behind an answer that had already been given.

use std::time::Instant;

use af_core::id::Uuid7;
use af_observability::verb_span::{OfferStage, VerbClock};
use af_wire::lease::LeasePayload;
use jiff::Timestamp;

use super::LeaseService;
use super::step::{Leased, Step};
use crate::error::Result;
use crate::lease::Offered;
use crate::lease::affinity::Fence;
use crate::lease::envelope::Acquired;

/// What a pass must let go of when it issues no lease.
struct Held {
    fleet_id: Uuid7,
    fence: Fence,
    event_id: Uuid7,
}

impl LeaseService {
    /// Runs a claimed pass, marking the admission, the issue and the offer's
    /// encoding on `clock`: the issue ends where `emit` is called.
    pub(super) async fn run_claimed_with<T, F>(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: Timestamp,
        began: Instant,
        clock: &mut VerbClock<OfferStage>,
        emit: &mut F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(LeasePayload<'a>, Offered) -> T + Send,
    {
        let held = Held {
            fleet_id: acquired.fleet_id.clone(),
            fence: acquired.fence,
            event_id: acquired.event_id.clone(),
        };
        clock.identify(held.event_id.as_str(), held.fleet_id.as_str());
        let admitted = self.admit_claimed(acquired, runner_id, now).await;
        clock.mark(OfferStage::Admit);
        let pass = match admitted.map(Step::proceed) {
            Ok(Ok(admitted)) => {
                let delivered = {
                    let mut timed = |lease: LeasePayload<'_>, offered: Offered| {
                        clock.mark(OfferStage::Issue);
                        let encoded = emit(lease, offered);
                        clock.mark(OfferStage::Encode);
                        encoded
                    };
                    self.deliver_with(runner_id, admitted, now, began, &mut timed)
                        .await
                };
                // The issue ends where `emit` is called. When it never is (a
                // refusal, or a failed issue as in a datastore outage), it ends
                // here, so that attempt's time is not left unexplained; a
                // stage already marked keeps its first reading.
                clock.mark(OfferStage::Issue);
                delivered
            }
            Ok(Err(ended)) => Ok(ended),
            Err(fault) => Err(fault),
        };
        match pass {
            Ok(Step::Go(Leased(answer))) => Ok(Some(answer)),
            Ok(Step::Stop) => {
                self.let_go(&held).await;
                Ok(None)
            }
            Ok(Step::Park) => {
                if let Err(fault) = self
                    .leases
                    .park(&held.fleet_id, &held.event_id, held.fence)
                    .await
                {
                    self.let_go(&held).await;
                    return Err(fault);
                }
                Ok(None)
            }
            Err(fault) => {
                self.let_go(&held).await;
                Err(fault)
            }
        }
    }

    /// Frees the claim through the one best-effort release every
    /// lease-less ending shares.
    async fn let_go(&self, held: &Held) {
        self.leases.let_go(&held.fleet_id, held.fence).await;
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use af_core::test_util::trace::Capture;

    use crate::lease::affinity::EVENT_CLAIM_RELEASE_FAILED;
    use crate::lease::test_dead;

    /// A fault after the claim reaches the caller as the fault, and the claim
    /// is still let go — here the release is refused too, so it is logged
    /// under the fault's own code and the claim lapses at its expiry instead.
    #[tokio::test]
    async fn should_raise_the_fault_and_log_a_release_the_datastore_refuses() {
        let log = Capture::install();
        let acquired = test_dead::acquired();
        let fleet = acquired.fleet_id.clone();

        let fault = test_dead::plane()
            .run_claimed_with(
                acquired,
                &test_dead::id(9),
                test_dead::AT,
                std::time::Instant::now(),
                &mut af_observability::verb_span::VerbClock::untimed(),
                &mut |_lease, _metadata| (),
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
