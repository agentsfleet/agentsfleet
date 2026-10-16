//! Preserve the typed placement decision until shared polling chooses an answer.

use afd_core::{clock::UnixMillis, id::Uuid7};
use afd_runner::capacity::choice::Ticket;

use super::Plane;
use crate::{Result, lease::answer::no_work};

/// A complete poll response with its placement decision preserved.
#[derive(Debug)]
pub enum Polled {
    /// A successful insertion owns a physical host worker.
    Lease(String),
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

impl Plane {
    /// Poll installed fleet work under an optional shared host choice.
    ///
    /// # Errors
    /// Preserves selection, policy, billing and durable issue failures.
    pub async fn poll(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        degraded: bool,
        now: UnixMillis,
        ticket: Option<Ticket>,
    ) -> Result<Polled> {
        let began = std::time::Instant::now();
        if degraded {
            return no_work(runner, "the runner's verdict is degraded or unreadable")
                .map(Polled::Empty);
        }
        let Some(acquired) = self.leases.select(runner, held, now).await? else {
            return no_work(runner, "no leasable work").map(Polled::Empty);
        };
        let mut chosen = self.clone();
        chosen.leases = chosen.leases.with_ticket(ticket);
        chosen.run_claimed(acquired, runner, now, began).await
    }
}
