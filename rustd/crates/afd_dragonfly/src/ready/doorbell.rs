//! The readiness doorbell: which partitions gained a mark since this
//! process's polls last looked.
//!
//! # Why a bell per partition
//!
//! A poll reads one partition per rotation step, so a mark in a partition the
//! cursor just passed waits for the rotation to come round: sixteen polls, up
//! to four seconds with four runner workers on the one-second back-off. One
//! shared hash naming the fresh partitions would fix that and put back the hot
//! slot partitioning removed, written by every mark and read by every poll.
//! So a mark rings its own partition's channel, on that partition's slot, and
//! every replica keeps what it heard as one bit per partition in its own
//! memory. A poll reads an atomic: nothing to expire, nothing to delete under
//! a token.
//!
//! # A missed ring is counted, not rung
//!
//! Pub/sub has no replay, and a gap or a lag cannot say which channels it
//! lost. Ringing all sixteen would send each poll to its own cursor's
//! partition, which the rotation reads anyway, and would hide every real ring
//! behind those bits for a full turn. So a gap is counted and
//! nothing is rung. A lost ring costs that fleet the rotation's speed and
//! never its work: the mark is already in its partition, and the rotation
//! visits every partition in turn.
//!
//! # Every command under a deadline
//!
//! The ring and the peek of a rung partition give up at
//! [`READY_WAKE_DEADLINE_MS`]: the doorbell exists to make a poll faster, so a
//! slow node may cost a mark its ring or a poll its rung peek, never the
//! datastore's request timeout.

use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

use afd_core::error_code::{self, Coded as _, Logged};
use afd_core::timing::READY_WAKE_DEADLINE_MS;
use afd_observability::metrics::label::fleet::ReadyWake;

use super::partition::{Partition, READY_PARTITIONS, ReadyPrefix};
use super::{CMD_HRANDFIELD, Ready, ReadyIndex, ReadyToken};
use crate::error::{self, Result};
use crate::hub::{Received, Subscription, SubscriptionHub};
use crate::streams::{CMD_PUBLISH, FleetStreams};

/// The `event` a mark logs when its ring was not published.
pub(crate) const EVENT_WAKE_RING_FAILED: &str = "ready_wake_ring_failed";

/// [`READY_WAKE_DEADLINE_MS`] as the duration a timeout takes.
const WAKE_DEADLINE: Duration = Duration::from_millis(READY_WAKE_DEADLINE_MS);

const _: () = assert!(
    READY_PARTITIONS as u32 == u16::BITS,
    "the doorbell keeps one bit per partition"
);

/// The partitions rung since this process's polls last took them.
///
/// Cheap to clone: every clone is the same sixteen bits, so the listener that
/// rings them and the polls that take them share one set.
#[derive(Debug, Clone, Default)]
pub struct Doorbell {
    rung: Arc<AtomicU16>,
}

impl Doorbell {
    /// A doorbell with nothing rung.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rings `partition`.
    pub fn ring(&self, partition: Partition) {
        self.rung.fetch_or(bit(partition), Ordering::AcqRel);
    }

    /// Takes the first rung partition at or after `from`, clearing its bit.
    ///
    /// Starting at the poll's cursor spreads concurrent polls over the rung
    /// partitions instead of sending them all to the lowest one. Two polls
    /// can see the same bit; `fetch_and` answers which of them cleared it, and
    /// the other looks again rather than trusting what it saw.
    #[must_use]
    pub fn take(&self, from: Partition) -> Option<Partition> {
        loop {
            let next = first_at_or_after(self.rung.load(Ordering::Acquire), from)?;
            if self.rung.fetch_and(!bit(next), Ordering::AcqRel) & bit(next) != 0 {
                return Some(next);
            }
        }
    }

    /// Whether `partition` is rung, without taking it: what a test waits on
    /// before the poll it means to send there.
    #[cfg(feature = "test-util")]
    #[must_use]
    pub fn is_rung(&self, partition: Partition) -> bool {
        self.rung.load(Ordering::Acquire) & bit(partition) != 0
    }

    /// Subscribes `hub` to every partition's channel under `prefix`, and
    /// rings this doorbell with what they hear for as long as the hub runs.
    pub fn listen(&self, hub: &SubscriptionHub, prefix: &ReadyPrefix) {
        for partition in Partition::all() {
            let subscription = hub.subscribe(&partition.wake_channel_under(prefix));
            tokio::spawn(answer(subscription, partition, self.clone()));
        }
    }
}

/// Rings `partition` for every message on its channel, and counts a gap or a
/// lag as a gap; ends when the hub shuts down.
async fn answer(mut subscription: Subscription, partition: Partition, doorbell: Doorbell) {
    while let Ok(received) = subscription.recv().await {
        match received {
            Received::Message(_) => doorbell.ring(partition),
            Received::Lagged(_) | Received::Gap => {
                afd_observability::producers::fleet::ready_wake_failed(ReadyWake::Gap);
            }
        }
    }
}

impl ReadyIndex {
    /// Rings `partition`'s channel for every replica's doorbell, carrying the
    /// `token` of the mark that rang it.
    ///
    /// The doorbell reads only the channel; the token is for whoever watches
    /// it, so a ring can be told apart from every other mark in its partition.
    ///
    /// Called by [`ReadyIndex::mark`] after its mark is written, and never
    /// raised from there: the mark is the record a poll needs, and a lost ring
    /// costs that fleet the rotation's speed, not its work. So a failure, or a
    /// publish past the deadline, is reported here, once, with the partition
    /// it concerned.
    pub(super) async fn ring(&self, partition: Partition, token: &ReadyToken) {
        let channel = partition.wake_channel_under(&self.prefix);
        let streams = FleetStreams::new(self.redis.clone());
        let published = within(CMD_PUBLISH, streams.publish(&channel, token.as_str())).await;
        if let Err(error) = published {
            report_unrung(partition, &error);
        }
    }

    /// Samples a partition a ring named, as [`ReadyIndex::peek`] does, but
    /// under the doorbell's deadline.
    ///
    /// # Errors
    /// Returns a timeout error past the deadline, and a command error when the
    /// read fails.
    pub async fn peek_rung(&self, partition: Partition, count: usize) -> Result<Vec<Ready>> {
        within(CMD_HRANDFIELD, self.peek(partition, count)).await
    }
}

/// `command`'s answer, or a timeout error naming it once the doorbell's
/// deadline passes.
async fn within<T>(command: &'static str, sent: impl Future<Output = Result<T>>) -> Result<T> {
    tokio::time::timeout(WAKE_DEADLINE, sent)
        .await
        .map_err(|_elapsed| error::timed_out(command, WAKE_DEADLINE.as_millis()))?
}

/// `partition`'s bit.
const fn bit(partition: Partition) -> u16 {
    1 << partition.index()
}

/// The first partition rung in `rung` at or after `from`, wrapping.
fn first_at_or_after(rung: u16, from: Partition) -> Option<Partition> {
    if rung == 0 {
        return None;
    }
    let ahead = rung.rotate_right(u32::from(from.index()));
    let offset = u16::try_from(ahead.trailing_zeros()).ok()?;
    Partition::new((from.index() + offset) % READY_PARTITIONS)
}

/// Reports a mark whose ring was not published.
fn report_unrung(partition: Partition, failure: &crate::Error) {
    afd_observability::producers::fleet::ready_wake_failed(ReadyWake::Ring);
    let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
    let index = partition.index();
    let Logged {
        error_code: cause_error_code,
        reason,
    } = failure.logged();
    tracing::warn!(
        error_code = code,
        cause_error_code,
        event = EVENT_WAKE_RING_FAILED,
        partition = index,
        reason,
        "a mark landed without its ring; the rotation still finds it"
    );
}

#[cfg(test)]
mod tests;
