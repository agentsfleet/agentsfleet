//! Preserve the typed placement decision until shared polling chooses an answer.

use afd_core::id::Uuid7;
use afd_runner::capacity::choice::Ticket;
use jiff::Timestamp;

use super::Plane;
use crate::Result;
use crate::lease::answer::{no_work, replay_now};
use crate::lease::assign::{Taken, record_polled};

/// A complete poll response with its placement decision preserved.
#[derive(Debug)]
pub enum Polled {
    /// A successful insertion owns a physical host worker.
    Lease(String),
    /// This fleet pass placed nothing, so another accepted kind may be tried.
    Empty(String),
    /// The poll repeated the key of a lease whose reply was lost, and that
    /// lease was let go. The answer stands: the worker polls again at once
    /// under a fresh key, so no other kind is tried on this one.
    Replayed(String),
}

impl Polled {
    /// The existing runner envelope, already serialized while policy was borrowed.
    #[must_use]
    pub fn into_body(self) -> String {
        match self {
            Self::Lease(body) | Self::Empty(body) | Self::Replayed(body) => body,
        }
    }
}

impl Plane {
    /// Poll installed fleet work under an optional shared host choice.
    ///
    /// `key` is the worker's `Idempotency-Key` for this poll attempt, when it
    /// sent one: a poll repeating the key of a lease it never read frees that
    /// lease instead of claiming.
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
        key: Option<Uuid7>,
    ) -> Result<Polled> {
        let began = std::time::Instant::now();
        if degraded {
            return no_work(runner, "the runner's verdict is degraded or unreadable")
                .map(Polled::Empty);
        }
        let mut chosen = self.clone();
        chosen.leases = chosen
            .leases
            .with_ticket(ticket)
            .with_poll_key(key)
            .with_fresh_tally();
        let polled = chosen.claim_and_run(runner, held, now, began).await;
        record_polled(chosen.leases.tally());
        polled
    }

    /// The claim, then the pass over whatever it took.
    async fn claim_and_run(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        now: Timestamp,
        began: std::time::Instant,
    ) -> Result<Polled> {
        let acquired = match self.leases.take(runner, held).await? {
            Taken::Work(acquired) => *acquired,
            // The worker never read the reply its earlier poll was sent, and
            // the lease that poll issued is let go: its next poll, under a
            // fresh key, takes the same admission back at once.
            Taken::Replayed => return replay_now(runner).map(Polled::Replayed),
            Taken::Nothing => return no_work(runner, "no leasable work").map(Polled::Empty),
        };
        self.run_claimed(acquired, runner, now, began).await
    }
}
