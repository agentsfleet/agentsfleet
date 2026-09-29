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

/// A claim that issued no lease could not be freed; it lapses at its expiry.
const EVENT_CLAIM_RELEASE_FAILED: &str = "lease_claim_release_failed";

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

    /// Frees the claim, best-effort.
    ///
    /// The pass already has its answer, and a release that fails changes
    /// nothing the runner can act on: the claim lapses at its expiry, which is
    /// the cost every stop paid before this existed. So the failure is logged
    /// and the answer stands.
    async fn let_go(&self, held: &Held, now: UnixMillis) {
        if let Err(failure) = self.leases.release(&held.fleet_id, held.fence, now).await {
            let code = failure.code().as_str();
            let fleet_id = held.fleet_id.as_str();
            let reason = failure.to_string();
            tracing::warn!(
                error_code = code,
                event = EVENT_CLAIM_RELEASE_FAILED,
                fleet_id,
                reason,
                "a claim that issued no lease was not freed; it lapses at its expiry"
            );
        }
    }
}
