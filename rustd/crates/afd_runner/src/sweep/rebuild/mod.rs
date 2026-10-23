//! Rebuild outbound delivery from durable obligations in a bounded number of passes.
//! Inbound admissions remain directly discoverable in PostgreSQL.

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
