//! A native claim selects fleet work while retaining the policy borrow.

use af_observability::verb_span::{OfferStage, VerbClock};
use af_wire::lease::LeasePayload;
use afd_runner::capacity::choice::Ticket;

use super::LeaseService;
use crate::{
    Result,
    lease::{OfferRequest, Offered, answer::no_work},
};

impl LeaseService {
    /// Place fleet work and invoke the sink while its borrowed policy is alive.
    /// The sink's result remains concrete and cannot borrow from that policy.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn claim_fleet_with<T, F>(
        &self,
        request: OfferRequest<'_>,
        ticket: Option<Ticket>,
        emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(LeasePayload<'a>, Offered) -> T + Send,
    {
        let mut untimed = VerbClock::untimed();
        self.claim_fleet_timed(request, ticket, &mut untimed, emit)
            .await
    }

    /// [`Self::claim_fleet_with`], marking each offer stage it reaches on
    /// `clock`, from the selection on.
    ///
    /// # Errors
    /// As [`Self::claim_fleet_with`].
    pub async fn claim_fleet_timed<T, F>(
        &self,
        request: OfferRequest<'_>,
        ticket: Option<Ticket>,
        clock: &mut VerbClock<OfferStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(LeasePayload<'a>, Offered) -> T + Send,
    {
        let began = std::time::Instant::now();
        if request.degraded {
            no_work(
                request.runner,
                "the runner's verdict is degraded or unreadable",
            );
            return Ok(None);
        }
        let Some(acquired) = self.leases.select(request.runner, request.held).await? else {
            no_work(request.runner, "no leasable work");
            return Ok(None);
        };
        clock.mark(OfferStage::Claim);
        let mut chosen = self.clone();
        chosen.leases = chosen.leases.with_ticket(ticket);
        let claimed = (acquired, request.runner, request.now, began);
        chosen
            .run_claimed_timed(claimed, clock, &mut emit)
            .await
    }
}
