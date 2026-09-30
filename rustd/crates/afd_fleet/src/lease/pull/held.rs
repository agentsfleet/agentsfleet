//! The claim a pass holds once it wins a fleet, and the one place it is let
//! go.
//!
//! Every ending that issues no lease frees the claim — a refusal, a retry, a
//! park, a fault — and it is freed here, after the pass has its answer, so a
//! stop added later cannot forget to. Each of those endings used to return
//! with the claim still held, and the fleet's next event waited out the
//! claim's whole lifetime behind an answer that had already been given.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_dragonfly::ReadyToken;

use super::Plane;
use super::step::{Leased, Step};
use crate::error::Result;
use crate::lease::affinity::Fence;
use crate::lease::envelope::Acquired;

/// What a pass must let go of when it issues no lease.
struct Held {
    fleet_id: Uuid7,
    fence: Fence,
    ready: ReadyToken,
}

impl Plane {
    /// Every step after a won claim, then what becomes of the claim.
    ///
    /// A lease keeps it. Anything else frees it, and a park also clears the
    /// fleet's mark with the generation the poll peeked, because the answer a
    /// person owes re-marks the fleet and nothing else will change before it.
    pub(super) async fn run_claimed(
        &self,
        acquired: Acquired,
        runner_id: &Uuid7,
        now: UnixMillis,
    ) -> Result<String> {
        let held = Held {
            fleet_id: acquired.fleet_id.clone(),
            fence: acquired.fence,
            ready: acquired.ready.clone(),
        };
        let pass = match self
            .admit_claimed(acquired, runner_id, now)
            .await
            .map(Step::proceed)
        {
            Ok(Ok(admitted)) => self.deliver(runner_id, admitted, now).await,
            Ok(Err(ended)) => Ok(ended),
            Err(fault) => Err(fault),
        };
        match pass {
            Ok(Step::Go(Leased(answer))) => Ok(answer),
            Ok(Step::Stop(answer)) => {
                self.let_go(&held, now).await;
                Ok(answer)
            }
            Ok(Step::Park(answer)) => {
                self.let_go(&held, now).await;
                self.leases.clear_mark(&held.fleet_id, &held.ready).await;
                Ok(answer)
            }
            Err(fault) => {
                self.let_go(&held, now).await;
                Err(fault)
            }
        }
    }

    /// Frees the claim through the one best-effort release every
    /// lease-less ending shares.
    async fn let_go(&self, held: &Held, now: UnixMillis) {
        self.leases.let_go(&held.fleet_id, held.fence, now).await;
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use afd_core::test_util::trace::Capture;

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
            .run_claimed(acquired, &test_dead::id(9), test_dead::AT)
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
