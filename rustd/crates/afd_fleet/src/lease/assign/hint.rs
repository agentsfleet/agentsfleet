//! The readiness directory's part in a poll: which partitions it peeks, and
//! when it spends the entry that sent it there.
//!
//! # Two peeks, every poll
//!
//! A poll reads the directory and peeks the rotation's next partition
//! together, then peeks one of the oldest dirty partitions when that is a
//! different one. The rotation peek is never skipped for the hint: it is what
//! reaches a mark whose directory entry was lost, and what keeps one busy
//! partition from taking every poll.
//!
//! Both peeks' fleets reach the candidate scan, the cursor's partition's
//! first, and the scan claims them first. A poll the rotation sends to a
//! partition with work leases that work, as it did before the directory, so
//! one rotation of polls still reaches every placeable fleet; the hint takes
//! what the cursor's partition leaves.
//!
//! # One of the oldest few, not the oldest
//!
//! Every poll reads the same directory, so sending each to the oldest entry
//! sends them all to one partition, where they race for its few fleets and all
//! but one lose their claim. Each poll is sent to one of the oldest
//! [`HINT_SPREAD`] instead, taken in turn as the cursor moves.
//!
//! # When an entry is spent
//!
//! A leased fleet keeps its mark for as long as it runs, so its partition
//! would stay dirty, and the oldest-first directory would keep sending polls
//! to work no runner can take. So an entry is deleted once a poll tried
//! its partition and could not lease from it: nothing leased at all, which
//! means every fleet offered was tried, or its only fleet leased, or its peek
//! failed. A poll that leased from the rotation's partition keeps the entry,
//! because the hinted fleets may never have been tried. The next mark there
//! writes a deleted entry again. The delete compares the token the poll read,
//! so a mark landing while the poll ran survives it.
//!
//! # Every read under the hint deadline
//!
//! The directory read and the hinted peek both give up at the hint deadline,
//! because most polls are sent to the same few entries: one sick node holding
//! them must cost each poll a tenth of a second, not the datastore's five. A hinted
//! peek that fails spends its entry, so the next poll is not sent back.

use afd_core::id::Uuid7;
use afd_dragonfly::{Dirty, Partition, Ready};
use futures_util::future::join;

use afd_observability::metrics::label::fleet::ReadyHint;

use super::diagnostics::{
    EVENT_HINT_DELETE_FAILED, EVENT_HINT_PEEK_FAILED, EVENT_HINT_READ_FAILED,
    EVENT_READY_PEEK_FAILED, warn_hint, warn_queue,
};
use super::{MAX_READY_CANDIDATES_PER_POLL, PollCost};
use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// How many of the oldest dirty partitions a poll may be sent to.
///
/// One sends every poll to the same partition. More than a few would send
/// polls to marks that landed after work which has waited longer.
const HINT_SPREAD: usize = 3;

/// The fleets one poll offers the candidate scan, the cursor's partition's
/// first.
#[derive(Debug)]
pub(super) struct Offer {
    /// Every fleet the two peeks found, the cursor's partition's first.
    pub(super) ready: Vec<Ready>,
    /// How many of `ready`, from the front, came from the cursor's partition.
    pub(super) cursor: usize,
}

impl Offer {
    /// An offer from one partition, which the cursor named.
    fn cursor_only(ready: Vec<Ready>) -> Self {
        let cursor = ready.len();
        Self { ready, cursor }
    }
}

/// The dirty partition a poll was sent to, and how many fleets its peek
/// found, `None` when the peek failed.
#[derive(Debug)]
pub(super) struct Hint {
    dirty: Dirty,
    found: Option<usize>,
}

impl Leases {
    /// The ready fleets this poll offers the candidate scan, and the hint
    /// that sent it to one of their partitions, if one did.
    ///
    /// # Errors
    /// Reports a rotation peek that failed when no hinted peek answered in its
    /// place. A directory read or a hinted peek that failed is logged and
    /// counted, and the rotation's fleets are offered alone.
    pub(super) async fn ready_this_poll(
        &self,
        runner_id: &Uuid7,
        cost: &mut PollCost,
    ) -> Result<(Offer, Option<Hint>)> {
        let index = self.ready();
        let partition = self.cursor().advance();
        let (directory, rotation) = join(
            index.dirty(),
            index.peek(partition, MAX_READY_CANDIDATES_PER_POLL),
        )
        .await;
        cost.dragonfly_commands += 2;
        let rotation =
            rotation.inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error));
        let sent_to = directory
            .inspect_err(|error| {
                warn_hint(
                    EVENT_HINT_READ_FAILED,
                    ReadyHint::Read,
                    runner_id,
                    None,
                    error,
                );
            })
            .ok()
            .and_then(|dirty| spread(dirty, partition));
        let Some(dirty) = sent_to else {
            return Ok((Offer::cursor_only(rotation?), None));
        };
        if dirty.partition == partition {
            // One key, already read: the rotation's answer is the hint's too.
            let ready = rotation?;
            let found = Some(ready.len());
            return Ok((Offer::cursor_only(ready), Some(Hint { dirty, found })));
        }
        cost.dragonfly_commands += 1;
        let hinted = index
            .peek_hinted(dirty.partition, MAX_READY_CANDIDATES_PER_POLL)
            .await
            .inspect_err(|error| {
                let named = Some(dirty.partition);
                warn_hint(
                    EVENT_HINT_PEEK_FAILED,
                    ReadyHint::Peek,
                    runner_id,
                    named,
                    error,
                );
            });
        let found = hinted.as_ref().ok().map(Vec::len);
        let hint = Some(Hint { dirty, found });
        match (hinted, rotation) {
            (Ok(hinted), Ok(rotation)) => Ok((cursor_first(rotation, hinted), hint)),
            // The rotation's partition is sick; the directory's still serves.
            (Ok(hinted), Err(_)) => Ok((
                Offer {
                    ready: hinted,
                    cursor: 0,
                },
                hint,
            )),
            (Err(_), Ok(rotation)) => Ok((Offer::cursor_only(rotation), hint)),
            (Err(_), Err(error)) => Err(error.into()),
        }
    }

    /// Deletes the hint's entry unless this poll should come back to it.
    ///
    /// Never raised: a delete that failed leaves a stale entry, which costs
    /// the next poll one peek of a partition this one already read.
    pub(super) async fn spend_hint(
        &self,
        runner_id: &Uuid7,
        hint: Option<Hint>,
        acquired: Option<&Acquired>,
        cost: &mut PollCost,
    ) {
        let Some(hint) = hint else {
            return;
        };
        let leased = acquired.map(|acquired| acquired.fleet_id.as_str());
        if keeps_entry(hint.dirty.partition, hint.found, leased) {
            return;
        }
        cost.dragonfly_commands += 1;
        if let Err(error) = self.ready().spend(&hint.dirty).await {
            let named = Some(hint.dirty.partition);
            warn_hint(
                EVENT_HINT_DELETE_FAILED,
                ReadyHint::Delete,
                runner_id,
                named,
                &error,
            );
        }
    }
}

/// Whether a dirty partition's entry outlives this poll.
///
/// `found` is how many fleets the hinted peek returned, `None` when it failed;
/// `leased` is the fleet this poll leased, if any.
fn keeps_entry(partition: Partition, found: Option<usize>, leased: Option<&str>) -> bool {
    let Some(found) = found else {
        // A failed peek: keeping the entry would send the next poll back.
        return false;
    };
    match leased {
        // Nothing leased: every fleet offered was tried, the hinted ones too.
        None => false,
        Some(fleet) if Partition::of(fleet) == partition => found > 1,
        // Leased from the rotation: the hinted fleets may never have been tried.
        Some(_) => true,
    }
}

/// The dirty partition this poll is sent to: one of the oldest
/// [`HINT_SPREAD`] in `dirty`, which is oldest first, chosen by the cursor's
/// `position`.
///
/// The cursor moves on every poll through this process, so consecutive polls
/// take the oldest few in turn; each replica turns its own, so polls through
/// different replicas spread the same way without coordinating.
fn spread(dirty: Vec<Dirty>, position: Partition) -> Option<Dirty> {
    let reach = dirty.len().min(HINT_SPREAD);
    let turn = usize::from(position.index()).checked_rem(reach)?;
    dirty.into_iter().nth(turn)
}

/// The cursor's partition's fleets, then the hint's, none dropped: each
/// peek returns at most the ceiling, so the scan reads at most twice that.
/// No fleet appears twice: the two come from different partitions, and a
/// fleet lives in one.
fn cursor_first(rotation: Vec<Ready>, hinted: Vec<Ready>) -> Offer {
    let cursor = rotation.len();
    let ready = rotation.into_iter().chain(hinted).collect();
    Offer { ready, cursor }
}

#[cfg(test)]
#[path = "hint/tests.rs"]
mod tests;
