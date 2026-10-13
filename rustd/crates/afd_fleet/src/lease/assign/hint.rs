//! The readiness directory's part in a poll: which partitions it peeks, and
//! when it spends the entry that sent it there.
//!
//! # Two peeks, every poll
//!
//! A poll reads the directory and peeks the rotation's next partition
//! together, then peeks the oldest dirty partition when that is a different
//! one. The rotation peek is never skipped for the hint: it is what reaches a
//! mark whose directory entry was lost, and what keeps one busy partition from
//! taking every poll.
//!
//! # When an entry is spent
//!
//! A leased fleet keeps its mark for as long as it runs, so its partition
//! would stay dirty, and the oldest-first directory would keep sending every
//! poll to work no runner can take. So an entry is deleted once a poll tried
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
//! because every poll is sent to the same oldest entry: one sick node there
//! must cost each poll a tenth of a second, not the datastore's five. A hinted
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
    ) -> Result<(Vec<Ready>, Option<Hint>)> {
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
        let oldest = directory
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
            .and_then(|dirty| dirty.into_iter().next());
        let Some(dirty) = oldest else {
            return Ok((rotation?, None));
        };
        if dirty.partition == partition {
            // One key, already read: the rotation's answer is the hint's too.
            let ready = rotation?;
            let found = Some(ready.len());
            return Ok((ready, Some(Hint { dirty, found })));
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
            (Ok(hinted), Ok(rotation)) => Ok((interleave(hinted, rotation), hint)),
            // The rotation's partition is sick; the directory's still serves.
            (Ok(hinted), Err(_)) => Ok((hinted, hint)),
            (Err(_), Ok(rotation)) => Ok((rotation, hint)),
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

/// `hinted` and `rotation` taken in turn, up to the poll's ceiling, so
/// neither partition's fleets can crowd the other's out of the candidate
/// scan. No fleet appears twice: the two come from different partitions,
/// and a fleet lives in one.
fn interleave(hinted: Vec<Ready>, rotation: Vec<Ready>) -> Vec<Ready> {
    let (mut hinted, mut rotation) = (hinted.into_iter(), rotation.into_iter());
    std::iter::from_fn(|| match (hinted.next(), rotation.next()) {
        (None, None) => None,
        (first, second) => Some(first.into_iter().chain(second)),
    })
    .flatten()
    .take(MAX_READY_CANDIDATES_PER_POLL)
    .collect()
}

#[cfg(test)]
#[path = "hint/tests.rs"]
mod tests;
