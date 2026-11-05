//! One claim statement per runner poll. The admission is the queue, and each
//! fleet's slot is its place in it.

use crate::{
    error::Result,
    lease::{
        envelope::{Acquired, Kind},
        store::Leases,
    },
};
use std::sync::atomic::{AtomicU64, Ordering};

use afd_core::id::Uuid7;
use afd_observability::{metrics::label::fleet::RunStart, producers};

mod ledger;

/// What one poll spent reaching the datastores.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PollCost {
    pub(crate) database_roundtrips: u64,
    pub(crate) dragonfly_commands: u64,
}

/// One poll's running cost, recorded as each step sends a statement or a
/// command.
///
/// Carried on the poll's own [`Leases`] clone, so every step of the pass adds
/// to the same count and the operator gauge reads what the poll did rather
/// than what it was expected to do. A call another store answers is counted
/// at its call site, one per statement it sends.
#[derive(Debug, Default)]
pub(crate) struct Tally {
    database: AtomicU64,
    dragonfly: AtomicU64,
}

impl Tally {
    /// `count` statements sent to Postgres.
    pub(crate) fn statements(&self, count: u64) {
        self.database.fetch_add(count, Ordering::Relaxed);
    }

    /// `count` commands sent to Dragonfly.
    pub(crate) fn commands(&self, count: u64) {
        self.dragonfly.fetch_add(count, Ordering::Relaxed);
    }

    /// Everything recorded so far.
    pub(crate) fn cost(&self) -> PollCost {
        PollCost {
            database_roundtrips: self.database.load(Ordering::Relaxed),
            dragonfly_commands: self.dragonfly.load(Ordering::Relaxed),
        }
    }
}

/// What the claim statement answered.
#[derive(Debug)]
pub(crate) enum Taken {
    /// A fleet claimed, its head stamped delivered and its event row open.
    Work(Box<Acquired>),
    /// The poll repeated the key of an earlier poll whose reply its worker
    /// never read. A lease that poll issued and that is still running is now
    /// let go, so its fleet is claimable at once; one that already ended is
    /// left as it is.
    Replayed,
    /// Nothing ready that this runner may take.
    Nothing,
}

impl Leases {
    /// Claim the oldest ready fleet this runner may take, in one statement
    /// that also stamps its head admission delivered and opens its event row.
    ///
    /// # Errors
    /// Preserves database failures and malformed stored identifiers.
    pub async fn select(&self, runner: &Uuid7, held: &[Uuid7]) -> Result<Option<Acquired>> {
        Ok(match self.take(runner, held).await? {
            Taken::Work(acquired) => Some(*acquired),
            Taken::Replayed | Taken::Nothing => None,
        })
    }

    /// [`Leases::select`], keeping a replayed poll apart from an empty one.
    ///
    /// The statement is counted on this store's tally; the poll records the
    /// whole pass's cost once it has its answer.
    pub(crate) async fn take(&self, runner: &Uuid7, held: &[Uuid7]) -> Result<Taken> {
        self.claim_counted(runner, held).await
    }
}

/// Records one finished poll's cost on the operator gauges.
pub(crate) fn record_polled(tally: &Tally) {
    let cost = tally.cost();
    producers::fleet::lease_polled(cost.database_roundtrips, cost.dragonfly_commands);
}

pub(crate) const fn started(kind: Kind) -> RunStart {
    match kind {
        Kind::Fresh => RunStart::Fresh,
        Kind::Reclaim => RunStart::Reclaimed,
    }
}

#[cfg(feature = "test-util")]
pub mod measured;
