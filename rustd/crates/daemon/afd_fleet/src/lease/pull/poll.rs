//! Preserve the typed placement decision until shared polling chooses an answer.

use af_core::id::Uuid7;
use af_wire::lease::LeasePayload;
use afd_runner::capacity::choice::Ticket;
use jiff::Timestamp;

use super::LeaseService;
use crate::{
    Result,
    lease::{OfferRequest, Offered, answer::no_work},
};

/// A complete poll response with its placement decision preserved.
#[derive(Debug)]
pub enum Polled<T = String> {
    /// A successful insertion owns a physical host worker.
    Lease(T),
    /// This fleet pass placed nothing, so another accepted kind may be tried.
    Empty,
}

impl LeaseService {
    /// Place fleet work and invoke the sink while its borrowed policy is alive.
    /// The sink's result remains concrete and cannot borrow from that policy.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn poll_with<T, F>(
        &self,
        request: OfferRequest<'_>,
        ticket: Option<Ticket>,
        mut emit: F,
    ) -> Result<Polled<T>>
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
            return Ok(Polled::Empty);
        }
        let Some(acquired) = self.leases.select(request.runner, request.held).await? else {
            no_work(request.runner, "no leasable work");
            return Ok(Polled::Empty);
        };
        let mut chosen = self.clone();
        chosen.leases = chosen.leases.with_ticket(ticket);
        chosen
            .run_claimed_with(acquired, request.runner, request.now, began, &mut emit)
            .await
    }
}
