//! The readiness doorbell's part in a poll: which partitions it peeks, and
//! when a rung partition is rung again.
//!
//! # Two peeks at most, sent together
//!
//! A poll peeks the rotation's next partition and, when the doorbell holds a
//! rung partition, the first one at or after the cursor as well, both at once.
//! The rotation peek is never skipped for the bell: it is what reaches a mark
//! whose ring was lost, and what keeps one busy partition from taking every
//! poll.
//!
//! Both peeks' fleets reach the candidate scan, the cursor's partition's
//! first, and the scan claims them first. A poll the rotation sends to a
//! partition with work leases that work, as it did before the doorbell, so one
//! rotation of polls still reaches every placeable fleet; the bell takes what
//! the cursor's partition leaves.
//!
//! # When a partition is rung again
//!
//! Taking a ring clears it, so the next poll is not sent back to work no
//! runner can take: a leased fleet keeps its mark for as long as it runs. The
//! poll rings the partition again, in process and with no Dragonfly command,
//! only when the candidate scan offered a fleet from it that this poll never
//! tried to claim, because it leased something ahead of it. A fleet the scan
//! left out, running or not this runner's labels, never holds the bell, and
//! every other exit leaves it clear, a faulted poll's included, so one broken
//! fleet cannot keep drawing every poll. A poll dropped before it decides, its
//! runner gone, rings the partition again rather than losing the ring.
//!
//! # The rung peek has a deadline; the rotation peek does not
//!
//! The rung peek gives up at the doorbell's deadline and counts as failed, so
//! a slow node holding the rung partition costs a poll the doorbell, never the
//! datastore's request timeout on top of its rotation.

use afd_core::id::Uuid7;
use afd_dragonfly::{Doorbell, Partition, Ready};
use futures_util::future::join;

use super::diagnostics::{EVENT_READY_PEEK_FAILED, warn_queue, warn_rung_peek};
use super::{MAX_READY_CANDIDATES_PER_POLL, PollCost};
use crate::error::Result;
use crate::lease::store::Leases;

/// The fleets one poll offers the candidate scan, the cursor's partition's
/// first.
#[derive(Debug)]
pub(super) struct Offer {
    /// Every fleet the peeks found, the cursor's partition's first.
    pub(super) ready: Vec<Ready>,
    /// How many of `ready`, from the front, came from the cursor's partition.
    pub(super) from_cursor: usize,
}

/// The partition the doorbell sent a poll to, its bit already taken.
///
/// Rings the partition again if dropped before the poll settles it: a poll
/// whose runner hung up mid-flight never decides, and losing the ring then
/// would leave the fleet to the rotation for no reason of its own.
#[derive(Debug)]
pub(super) struct Rung {
    partition: Partition,
    doorbell: Doorbell,
    settled: bool,
}

impl Rung {
    fn new(partition: Partition, doorbell: &Doorbell) -> Self {
        Self {
            partition,
            doorbell: doorbell.clone(),
            settled: false,
        }
    }
}

impl Drop for Rung {
    fn drop(&mut self) {
        if !self.settled {
            self.doorbell.ring(self.partition);
        }
    }
}

impl Leases {
    /// The ready fleets this poll offers the candidate scan, and the rung
    /// partition that sent it to one of them, if the doorbell did.
    ///
    /// The rung partition comes back whatever the offer did, so the poll
    /// settles it on every exit. The offer is an error when the rotation peek
    /// failed and no rung peek answered in its place. A rung peek that failed
    /// or ran past the deadline is logged and counted, and the rotation's
    /// fleets are offered alone.
    pub(super) async fn ready_this_poll(
        &self,
        runner_id: &Uuid7,
        cost: &mut PollCost,
    ) -> (Result<Offer>, Option<Rung>) {
        let partition = self.cursor().advance();
        let rung = self
            .doorbell()
            .take(partition)
            .map(|taken| Rung::new(taken, self.doorbell()));
        let other = rung
            .as_ref()
            .map(|rung| rung.partition)
            .filter(|taken| *taken != partition);
        let offer = match other {
            Some(other) => self.peek_both(runner_id, partition, other, cost).await,
            // No ring, or the rotation's own partition rang: one key, one read.
            None => self
                .peek_rotation(runner_id, partition, cost)
                .await
                .map(|ready| cursor_first(ready, Vec::new()))
                .map_err(Into::into),
        };
        (offer, rung)
    }

    /// Rings `rung` again when the scan offered a fleet from it that this poll
    /// never tried, and settles it either way.
    ///
    /// No Dragonfly command: the bit is this process's own, and only its
    /// later polls read it.
    pub(super) fn ring_again(&self, rung: Option<Rung>, untried: &[Uuid7]) {
        let Some(mut rung) = rung else {
            return;
        };
        if offered_untried(rung.partition, untried) {
            self.doorbell().ring(rung.partition);
        }
        rung.settled = true;
    }

    /// The rotation's partition alone.
    async fn peek_rotation(
        &self,
        runner_id: &Uuid7,
        partition: Partition,
        cost: &mut PollCost,
    ) -> afd_dragonfly::error::Result<Vec<Ready>> {
        cost.dragonfly_commands += 1;
        self.ready()
            .peek(partition, MAX_READY_CANDIDATES_PER_POLL)
            .await
            .inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error))
    }

    /// The rotation's partition and the rung one, sent together, the rung
    /// one under the doorbell's deadline.
    async fn peek_both(
        &self,
        runner_id: &Uuid7,
        partition: Partition,
        rung: Partition,
        cost: &mut PollCost,
    ) -> Result<Offer> {
        let index = self.ready();
        cost.dragonfly_commands += 2;
        let (rotation, rung_peek) = join(
            index.peek(partition, MAX_READY_CANDIDATES_PER_POLL),
            index.peek_rung(rung, MAX_READY_CANDIDATES_PER_POLL),
        )
        .await;
        let rotation =
            rotation.inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error));
        let rung_peek = rung_peek.inspect_err(|error| warn_rung_peek(runner_id, rung, error));
        match (rotation, rung_peek) {
            // Neither answered, so the poll cannot say there is no work.
            (Err(error), Err(_)) => Err(error.into()),
            // Whichever answered is offered, the rotation's fleets first.
            (rotation, rung_peek) => Ok(cursor_first(
                rotation.unwrap_or_default(),
                rung_peek.unwrap_or_default(),
            )),
        }
    }
}

/// Whether `untried`, the fleets the scan offered that this poll never tried
/// to claim, holds one from `partition`.
fn offered_untried(partition: Partition, untried: &[Uuid7]) -> bool {
    untried
        .iter()
        .any(|fleet| Partition::of(fleet.as_str()) == partition)
}

/// The cursor's partition's fleets, then the rung one's, none dropped: each
/// peek returns at most the ceiling, so the scan reads at most twice that.
/// No fleet appears twice: the two come from different partitions, and a
/// fleet lives in one.
fn cursor_first(rotation: Vec<Ready>, rung: Vec<Ready>) -> Offer {
    let from_cursor = rotation.len();
    let ready = rotation.into_iter().chain(rung).collect();
    Offer { ready, from_cursor }
}

#[cfg(test)]
#[path = "wake/tests.rs"]
mod tests;
