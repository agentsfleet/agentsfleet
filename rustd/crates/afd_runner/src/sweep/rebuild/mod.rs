//! Every recovery pass, run a stated number of rounds, for a rebuild.
//!
//! # What a rebuild is
//!
//! PostgreSQL is the forge and Dragonfly a cache of it, and the claim is
//! only true if the cache can be thrown away and refilled from the ledger.
//! The refilling is not new code: admission replay re-appends what was
//! accepted and never queued, admission reconcile forgets receipts the
//! stream cannot produce, the outbound producer re-appends answers still
//! owed, and reclaim re-marks fleets that hold work. Each already runs as a
//! sweeper on its own interval. A rebuild is those same passes, run to a
//! bounded end in one place — the routine the flush proof runs, and the tool
//! an operator reaches for after tearing the cluster down.
//!
//! # Why rounds and not "until nothing changed"
//!
//! The obvious loop has no fixed point here. Reclaim re-marks a fleet on
//! every pass for as long as its stream holds deliverable work, and that is
//! correct — the mark is a hint the poll path consumes, and re-issuing it
//! costs nothing — so its tally never reaches zero while work is queued, and a
//! rebuild waiting for that would wait until runners drained the very queue
//! it was refilling. So a rebuild runs the rounds it is given and reports what
//! they did. The proof that it was COMPLETE belongs to the caller: the flush
//! test asserts every seeded job reaches terminal, and an operator reads the
//! capacity report. Neither takes this module's tally as the verdict.
//!
//! # Why the order within a round is the caller's
//!
//! Reconcile voids a receipt the stream lost, which returns the row to the
//! set replay re-appends, so replay belongs after reconcile in a round and
//! the next round's replay catches what this one's reconcile voided; reclaim
//! belongs last, so it marks fleets whose streams the earlier passes just
//! refilled. That order is knowledge about the sweepers, not about the loop,
//! and it is written where the sweepers are composed.

use std::pin::Pin;

use crate::error::Result;
use crate::sweep::{Sweep, Swept};

/// One recovery pass, callable without knowing which sweeper it is.
///
/// [`Sweep::sweep`] answers `impl Future`, which keeps the shared loop
/// monomorphic and keeps the trait out of a `dyn`. A rebuild wants a LIST of
/// unlike sweepers, so this is the object-safe face of the same pass: the
/// future is boxed once per call, on a path that runs a handful of times per
/// rebuild rather than per poll. The blanket impl below means every sweeper
/// is already a `Pass` and none has to say so.
pub trait Pass: Send + Sync {
    /// What this pass is called, in the line that reports it.
    fn name(&self) -> &'static str;

    /// Performs one pass.
    ///
    /// # Errors
    /// Whatever the sweeper's own pass reports.
    fn pass(&self) -> Pin<Box<dyn Future<Output = Result<Swept>> + Send + '_>>;
}

impl<S: Sweep> Pass for S {
    fn name(&self) -> &'static str {
        Sweep::name(self)
    }

    fn pass(&self) -> Pin<Box<dyn Future<Output = Result<Swept>> + Send + '_>> {
        Box::pin(self.sweep())
    }
}

/// What a rebuild did, summed over every pass of every round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rebuilt {
    /// Rounds run — the number asked for, since a rebuild runs them all.
    pub rounds: u32,
    /// Rows every pass considered.
    pub scanned: u64,
    /// Rows every pass changed.
    pub changed: u64,
}

/// Runs each pass in order, `rounds` times over, and sums what they did.
///
/// A pass that fails ends the rebuild there, with the error: a rebuild that
/// skipped a pass is a cache refilled from part of the ledger, which is the
/// state this exists to make impossible. Zero rounds runs nothing and answers
/// zeros, which is a caller asking for no rebuild and getting one.
///
/// # Errors
/// The first pass that would not complete.
pub async fn rebuild(passes: &[&dyn Pass], rounds: u32) -> Result<Rebuilt> {
    let mut tally = Rebuilt {
        rounds,
        ..Rebuilt::default()
    };
    for round in 0..rounds {
        for pass in passes {
            let swept = pass.pass().await?;
            tally.scanned = tally.scanned.saturating_add(swept.scanned);
            tally.changed = tally.changed.saturating_add(swept.changed);
            let sweeper = pass.name();
            tracing::debug!(
                round,
                sweeper,
                scanned = swept.scanned,
                changed = swept.changed,
                event = "rebuild_pass_completed",
            );
        }
    }
    let (rounds, scanned, changed) = (tally.rounds, tally.scanned, tally.changed);
    tracing::info!(
        rounds,
        scanned,
        changed,
        event = "rebuild_completed",
        "every recovery pass has run its rounds; completeness is the caller's to assert"
    );
    Ok(tally)
}

#[cfg(test)]
mod tests;
