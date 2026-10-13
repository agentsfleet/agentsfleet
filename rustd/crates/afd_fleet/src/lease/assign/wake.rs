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
//! only while others there may still be waiting: it leased from the
//! rotation's partition while the rung peek found fleets the scan may never
//! have tried, or it leased from the rung partition while its peek held
//! others. Every other exit leaves it clear, a faulted poll's included, so one
//! broken fleet cannot keep drawing every poll. When the rotation's partition
//! fills the scan's ceiling with fleets none of which leases, stale marks
//! after a burst say, the rung ones are cut untried and the ring still goes;
//! the rotation reaches them within a turn, and the next mark there rings
//! again.

use afd_core::id::Uuid7;
use afd_dragonfly::{Partition, Ready};
use futures_util::future::join;

use super::diagnostics::{EVENT_READY_PEEK_FAILED, warn_queue, warn_rung_peek};
use super::{MAX_READY_CANDIDATES_PER_POLL, PollCost};
use crate::error::Result;
use crate::lease::envelope::Acquired;
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

impl Offer {
    /// An offer from one partition, which the cursor named.
    fn cursor_only(ready: Vec<Ready>) -> Self {
        let from_cursor = ready.len();
        Self { ready, from_cursor }
    }
}

/// The partition the doorbell sent a poll to, and how many fleets its peek
/// found, `None` when the peek failed.
#[derive(Debug)]
pub(super) struct Rung {
    pub(super) partition: Partition,
    pub(super) found: Option<usize>,
}

impl Leases {
    /// The ready fleets this poll offers the candidate scan, and the rung
    /// partition that sent it to one of them, if the doorbell did.
    ///
    /// The rung partition comes back whatever the offer did, so the poll
    /// decides on every exit whether to ring it again. The offer is an error
    /// when the rotation peek failed and no rung peek answered in its place.
    /// A rung peek that failed is logged and counted, and the rotation's
    /// fleets are offered alone.
    pub(super) async fn ready_this_poll(
        &self,
        runner_id: &Uuid7,
        cost: &mut PollCost,
    ) -> (Result<Offer>, Option<Rung>) {
        let partition = self.cursor().advance();
        match self.doorbell().take(partition) {
            Some(rung) if rung != partition => {
                self.peek_both(runner_id, partition, rung, cost).await
            }
            rung => {
                let rotation = self.peek_rotation(runner_id, partition, cost).await;
                // The rotation's own partition rang: one key, already read.
                let found = rotation.as_ref().ok().map(Vec::len);
                let rung = rung.map(|partition| Rung { partition, found });
                (rotation.map(Offer::cursor_only).map_err(Into::into), rung)
            }
        }
    }

    /// Rings `rung` again when this poll should come back to it.
    ///
    /// No Dragonfly command: the bit is this process's own, and only its
    /// later polls read it.
    pub(super) fn ring_again(&self, rung: Option<Rung>, acquired: Option<&Acquired>) {
        let Some(rung) = rung else {
            return;
        };
        let leased = acquired.map(|acquired| acquired.fleet_id.as_str());
        if rings_again(rung.partition, rung.found, leased) {
            self.doorbell().ring(rung.partition);
        }
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

    /// The rotation's partition and the rung one, sent together.
    async fn peek_both(
        &self,
        runner_id: &Uuid7,
        partition: Partition,
        rung: Partition,
        cost: &mut PollCost,
    ) -> (Result<Offer>, Option<Rung>) {
        let index = self.ready();
        cost.dragonfly_commands += 2;
        let (rotation, rung_peek) = join(
            index.peek(partition, MAX_READY_CANDIDATES_PER_POLL),
            index.peek(rung, MAX_READY_CANDIDATES_PER_POLL),
        )
        .await;
        let rotation =
            rotation.inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error));
        let rung_peek = rung_peek.inspect_err(|error| warn_rung_peek(runner_id, rung, error));
        let found = rung_peek.as_ref().ok().map(Vec::len);
        let offer = match (rung_peek, rotation) {
            (Ok(rung_peek), Ok(rotation)) => Ok(cursor_first(rotation, rung_peek)),
            // The rotation's partition is sick; the rung one still serves.
            (Ok(rung_peek), Err(_)) => Ok(Offer {
                ready: rung_peek,
                from_cursor: 0,
            }),
            (Err(_), Ok(rotation)) => Ok(Offer::cursor_only(rotation)),
            (Err(_), Err(error)) => Err(error.into()),
        };
        (
            offer,
            Some(Rung {
                partition: rung,
                found,
            }),
        )
    }
}

/// Whether a rung partition is rung again after this poll.
///
/// `found` is how many fleets the rung peek returned, `None` when it failed;
/// `leased` is the fleet this poll leased, if any.
fn rings_again(partition: Partition, found: Option<usize>, leased: Option<&str>) -> bool {
    let Some(found) = found else {
        // A failed peek: ringing again would send the next poll back to it.
        return false;
    };
    if found == 0 {
        // Nothing there to come back for, wherever the poll leased.
        return false;
    }
    match leased {
        // Nothing leased: every fleet offered was tried, unless the rotation's
        // filled the scan; then the rotation reaches the rung ones.
        None => false,
        Some(fleet) if Partition::of(fleet) == partition => found > 1,
        // Leased from the rotation: the rung fleets may never have been tried.
        Some(_) => true,
    }
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
