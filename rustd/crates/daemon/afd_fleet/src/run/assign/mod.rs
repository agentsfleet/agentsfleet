//! One bounded Postgres discovery pass per gateway claim. The admission is the queue.

use crate::{
    error::Result,
    run::{
        assignment::RecoveryScan,
        envelope::{Acquired, Kind},
        store::Runs,
    },
};
use af_core::id::Uuid7;
use af_observability::{metrics::label::fleet::RunStart, producers};

mod ledger;

/// Candidate ceiling shared by discovery and its measurement.
pub const MAX_CANDIDATES_PER_POLL: usize = 64;

#[derive(Debug, Default)]
struct PollCost {
    candidates_scanned: u64,
}

impl Runs {
    /// Select the oldest eligible admission of one fleet, fenced with its slot.
    /// Each triggered claim probes Postgres; the daemon's recovery tick covers lost notifications.
    ///
    /// # Errors
    /// Preserves database failures and malformed stored identifiers.
    pub async fn select(&self, runner: &Uuid7, held: &[Uuid7]) -> Result<Option<Acquired>> {
        self.select_ordered(runner, held, None).await
    }

    pub(crate) async fn select_ordered(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        recovery: Option<&mut RecoveryScan>,
    ) -> Result<Option<Acquired>> {
        self.select_recording(runner, held, recovery).await.0
    }

    async fn select_recording(
        &self,
        runner: &Uuid7,
        held: &[Uuid7],
        recovery: Option<&mut RecoveryScan>,
    ) -> (Result<Option<Acquired>>, PollCost) {
        let mut cost = PollCost::default();
        let selected = self.ledger_counted(runner, held, recovery, &mut cost).await;
        producers::fleet::run_polled(cost.candidates_scanned);
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
