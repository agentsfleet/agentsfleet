//! A native claim selects fleet work while retaining the policy borrow.

use af_observability::verb_span::{OfferStage, VerbClock};
use af_wire::run::RunPayload;
use afd_runner::capacity::choice::Ticket;

use super::RunService;
use crate::{
    Result,
    run::{OfferRequest, Offered, answer::no_work},
};

impl RunService {
    /// Place fleet work and invoke the sink while its borrowed policy is alive.
    /// The sink's result remains concrete and cannot borrow from that policy.
    /// Marks each offer stage it reaches on `clock`, from the selection on.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn claim_fleet_with<T, F>(
        &self,
        request: &mut OfferRequest<'_>,
        ticket: Option<Ticket>,
        clock: &mut VerbClock<OfferStage>,
        mut emit: F,
    ) -> Result<Option<T>>
    where
        T: Send,
        F: for<'a> FnMut(RunPayload<'a>, Offered) -> T + Send,
    {
        let began = std::time::Instant::now();
        if request.degraded {
            no_work(
                request.runner,
                "the runner's verdict is degraded or unreadable",
            );
            return Ok(None);
        }
        let Some(acquired) = self
            .runs
            .select_ordered(
                request.runner,
                request.held,
                request.recovery.as_deref_mut(),
            )
            .await?
        else {
            no_work(request.runner, "no leasable work");
            return Ok(None);
        };
        clock.mark(OfferStage::Claim);
        let mut chosen = self.clone();
        chosen.runs = chosen.runs.with_ticket(ticket);
        chosen
            .run_claimed_with(
                acquired,
                request.runner,
                request.now,
                began,
                clock,
                &mut emit,
            )
            .await
    }
}
