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
//! poll to work no runner can take. So an entry is deleted once a poll could
//! not lease from its partition, and kept only when this poll leased from it
//! and its peek held other fleets too. The next mark there writes it again.
//! The delete compares the token the poll read, so a mark landing while the
//! poll ran survives it.

use afd_core::id::Uuid7;
use afd_dragonfly::{Dirty, Partition, Ready};
use futures_util::future::join;

use super::diagnostics::{
    EVENT_READY_PEEK_FAILED, warn_hint_unread, warn_hint_unspent, warn_queue,
};
use super::{MAX_READY_CANDIDATES_PER_POLL, PollCost};
use crate::error::Result;
use crate::lease::envelope::Acquired;
use crate::lease::store::Leases;

/// The dirty partition a poll was sent to, and how many fleets its peek found.
#[derive(Debug)]
pub(super) struct Hint {
    dirty: Dirty,
    found: usize,
}

impl Leases {
    /// The ready fleets this poll offers the candidate scan, and the hint
    /// that sent it to one of their partitions, if one did.
    ///
    /// # Errors
    /// Reports a rotation peek that failed, as the poll always has. A
    /// directory or a hinted peek that failed is logged and skipped: the
    /// rotation's fleets are still offered.
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
            rotation.inspect_err(|error| warn_queue(EVENT_READY_PEEK_FAILED, runner_id, error))?;
        let oldest = directory
            .inspect_err(|error| warn_hint_unread(runner_id, None, error))
            .ok()
            .and_then(|dirty| dirty.into_iter().next());
        let Some(dirty) = oldest else {
            return Ok((rotation, None));
        };
        if dirty.partition == partition {
            let found = rotation.len();
            return Ok((rotation, Some(Hint { dirty, found })));
        }
        cost.dragonfly_commands += 1;
        match index
            .peek(dirty.partition, MAX_READY_CANDIDATES_PER_POLL)
            .await
        {
            Ok(hinted) => {
                let found = hinted.len();
                Ok((interleave(hinted, rotation), Some(Hint { dirty, found })))
            }
            Err(error) => {
                warn_hint_unread(runner_id, Some(dirty.partition), &error);
                Ok((rotation, None))
            }
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
            warn_hint_unspent(runner_id, hint.dirty.partition, &error);
        }
    }
}

/// Whether a dirty partition's entry outlives this poll: only when the poll
/// leased from it and its peek held other fleets as well.
fn keeps_entry(partition: Partition, found: usize, leased: Option<&str>) -> bool {
    found > 1 && leased.is_some_and(|fleet| Partition::of(fleet) == partition)
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
