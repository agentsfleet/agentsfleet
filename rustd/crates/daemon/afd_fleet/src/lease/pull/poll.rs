//! Preserve the typed placement decision until shared polling chooses an answer.

use af_core::id::Uuid7;
use afd_runner::capacity::choice::Ticket;
use jiff::Timestamp;
use af_wire::lease::LeasePayload;

use super::LeaseService;
use crate::{Result, lease::{OfferRequest, Offered, answer::{answer, no_work}}};

/// A complete poll response with its placement decision preserved.
#[derive(Debug)]
pub enum Polled<T = String> {
    /// A successful insertion owns a physical host worker.
    Lease(T),
    /// This fleet pass placed nothing, so another accepted kind may be tried.
    Empty(String),
}

impl Polled {
    /// The existing runner envelope, already serialized while policy was borrowed.
    #[must_use]
    pub fn into_body(self) -> String {
        match self {
            Self::Lease(body) | Self::Empty(body) => body,
        }
    }
}

impl LeaseService {
    /// Poll installed fleet work under an optional shared host choice.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn poll(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        degraded: bool,
        now: Timestamp,
        ticket: Option<Ticket>,
    ) -> Result<Polled> {
        let request = OfferRequest { runner, held, degraded, now };
        let rendered = self.poll_with(request, ticket, |payload, _| answer(Some(payload))).await?;
        match rendered {
            Polled::Lease(body) => body.map(Polled::Lease),
            Polled::Empty(body) => Ok(Polled::Empty(body)),
        }
    }

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
            return no_work(request.runner, "the runner's verdict is degraded or unreadable")
                .map(Polled::Empty);
        }
        let Some(acquired) = self.leases.select(request.runner, request.held).await? else {
            return no_work(request.runner, "no leasable work").map(Polled::Empty);
        };
        let mut chosen = self.clone();
        chosen.leases = chosen.leases.with_ticket(ticket);
        chosen.run_claimed_with(acquired, request.runner, request.now, began, &mut emit).await
    }
}
