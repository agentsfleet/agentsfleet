//! Wall viewers reading the fleet live set beside the saturated poll.
//!
//! A live wall re-asks `Fleets::live_set` every tick for as long as a tab is
//! open, on the same pool the runner verbs use. Each reader here is one such
//! viewer cycling through every chat tenant's workspace; whenever an entry
//! expires, every reader on that workspace misses at once, which is the
//! instant the live-set load coalescing exists for.

use core::time::Duration;
use std::time::Instant;

use afd_core::id::Uuid7;
use afd_fleet_lifecycle::Fleets;

use super::requester::Shared;
use crate::error::Result;
use crate::report::Latency;

/// Viewers reading at once.
pub(super) const READERS: usize = 8;

/// The pause between one viewer's reads: a wall ticks, it does not spin.
const READ_EVERY: Duration = Duration::from_millis(50);

/// What a viewer reads a workspace's live set through.
pub(super) trait Wall: Sync {
    /// Read `workspace`'s set once, answering whether the path answered.
    fn read(&self, workspace: &Uuid7) -> impl Future<Output = bool> + Send;
}

impl Wall for Fleets {
    async fn read(&self, workspace: &Uuid7) -> bool {
        self.live_set(workspace).await.is_ok()
    }
}

/// What the viewers saw.
#[derive(Debug)]
pub(super) struct Reads {
    /// How long one read took, cached or not.
    pub(super) latency: Latency,
    /// Reads the path refused.
    pub(super) refused: u64,
}

impl Reads {
    /// No reads yet.
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            latency: Latency::new()?,
            refused: 0,
        })
    }

    /// Fold another viewer's reads into these.
    ///
    /// # Errors
    ///
    /// When the two latency distributions will not merge.
    pub(super) fn absorb(&mut self, other: &Self) -> Result<()> {
        self.refused += other.refused;
        self.latency.merge(&other.latency)
    }
}

/// Read every workspace's live set in turn while the window is open.
///
/// # Errors
///
/// A latency the histogram would not hold. Never a refusal from the path,
/// which is counted.
pub(super) async fn view<W: Wall>(
    wall: &W,
    workspaces: &[Uuid7],
    shared: &Shared,
) -> Result<Reads> {
    let mut reads = Reads::new()?;
    for workspace in workspaces.iter().cycle() {
        if !shared.open() {
            break;
        }
        let started = Instant::now();
        if wall.read(workspace).await {
            reads.latency.record(started.elapsed())?;
        } else {
            reads.refused += 1;
        }
        tokio::time::sleep(READ_EVERY).await;
    }
    Ok(reads)
}

#[cfg(test)]
mod tests;
