//! One bounded Postgres discovery pass per runner poll. The admission is the queue.

use crate::{
    error::Result,
    lease::{
        envelope::{Acquired, Kind},
        store::Leases,
    },
};
use afd_core::id::Uuid7;
use afd_observability::{metrics::label::fleet::RunStart, producers};

mod ledger;

/// Candidate ceiling shared by discovery and its measurement.
pub const MAX_CANDIDATES_PER_POLL: usize = 64;

#[derive(Debug, Default)]
struct PollCost {
    candidates_scanned: u64,
    database_roundtrips: u64,
}

impl Leases {
    /// Select the oldest eligible admission of one fleet, fenced with its slot.
    /// Every poll probes Postgres, including when every notification was lost.
    ///
    /// # Errors
    /// Preserves database failures and malformed stored identifiers.
    pub async fn select(&self, runner: &Uuid7, held: &[Uuid7]) -> Result<Option<Acquired>> {
        self.select_recording(runner, held).await.0
    }

    async fn select_recording(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
    ) -> (Result<Option<Acquired>>, PollCost) {
        let mut cost = PollCost::default();
        let selected = self.ledger_counted(runner, held, &mut cost).await;
        producers::fleet::lease_polled(cost.candidates_scanned, cost.database_roundtrips, 0);
        (selected, cost)
    }
}

pub(crate) const fn started(kind: Kind) -> RunStart {
    match kind {
        Kind::Fresh => RunStart::Fresh,
        Kind::Reclaim => RunStart::Reclaimed,
    }
}

#[cfg(feature = "test-util")]
pub mod measured;
