//! Sharded notifications that end held heartbeats; Postgres remains the queue.
//! A ring carries no work and may be lost at any time without losing an
//! admission: the runner's next poll still claims it.

use crate::{
    Dragonfly, FleetStreams, SubscriptionHub,
    hub::{Received, Subscription},
};
use afd_core::{
    error_code::{Coded as _, Logged},
    id::Uuid7,
    timing::READY_WAKE_DEADLINE,
};
use afd_observability::{metrics::label::fleet::ReadyWake, producers};
use std::{
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

/// Independent publish channels spread notifications over cluster slots.
pub const WAKE_SHARDS: u16 = 16;
const CHANNEL_PREFIX: &str = "fleet:wake";
const EVENT_RING_FAILED: &str = "admission_wake_ring_failed";

/// The rings a held heartbeat waits for, shared by the hub listener and every
/// held beat on this replica.
#[derive(Debug, Clone, Default)]
pub struct Doorbell {
    rings: Arc<Rings>,
}

/// One ring wakes one held beat, and how many beats are held waiting.
///
/// A `Notify` rather than a broadcast: one admission is one unit of work, and
/// the oldest-first claim serves whichever host polls, so waking every idle
/// host on the replica would send them all to claim the one admission. A ring
/// with no beat held stores one permit, so the next beat returns at once and
/// no ring that lands between two holds is lost.
#[derive(Debug, Default)]
struct Rings {
    notify: Notify,
    held: AtomicUsize,
    /// Set once the process stops accepting requests: no beat is held after.
    closed: AtomicBool,
}

/// How a held beat ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// A ring arrived: fresh work was admitted somewhere.
    Rung,
    /// The deadline passed with no ring.
    Deadline,
    /// `max` beats were already held, so this one was not.
    Capped,
}

/// One held beat's place under the cap, given back when the wait ends or the
/// request is dropped mid-wait.
struct Held<'a>(&'a AtomicUsize);

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Doorbell {
    /// A doorbell nothing has rung.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Fresh work was admitted: complete the longest-held beat on this
    /// replica, or leave one permit for the next beat to arrive.
    ///
    /// No fleet is named. The claim takes the oldest ready fleet, so the woken
    /// host needs no hint to find it.
    pub fn ring(&self) {
        self.rings.notify.notify_one();
    }

    /// Holds the caller until the next ring or `deadline`, unless `max` beats
    /// are already held.
    ///
    /// The wait is registered before the closed check, so a close between the
    /// two still reaches it, and a ring between the two is kept as a permit.
    pub async fn wait_for_ring(&self, deadline: Duration, max: usize) -> Waited {
        let reserved = self
            .rings
            .held
            .try_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                (held < max).then_some(held + 1)
            });
        if reserved.is_err() {
            return Waited::Capped;
        }
        let _held = Held(&self.rings.held);
        let mut rung = pin!(self.rings.notify.notified());
        rung.as_mut().enable();
        if self.is_closed() {
            return Waited::Deadline;
        }
        match tokio::time::timeout(deadline, rung).await {
            Ok(()) if self.is_closed() => Waited::Deadline,
            Ok(()) => Waited::Rung,
            Err(_) => Waited::Deadline,
        }
    }

    /// Ends every held wait at once and holds no more, for a process that has
    /// stopped accepting requests: a held beat would otherwise keep its drain
    /// waiting until its deadline. The waits answer as at their deadline.
    pub fn close(&self) {
        self.rings.closed.store(true, Ordering::Release);
        self.rings.notify.notify_waiters();
    }

    fn is_closed(&self) -> bool {
        self.rings.closed.load(Ordering::Acquire)
    }

    /// How many beats are held right now.
    #[must_use]
    pub fn held(&self) -> usize {
        self.rings.held.load(Ordering::Acquire)
    }

    /// Listen through the process's existing subscription hub.
    pub fn listen(&self, hub: &SubscriptionHub) {
        for shard in 0..WAKE_SHARDS {
            tokio::spawn(answer(hub.subscribe(&channel(shard)), self.clone()));
        }
    }
}

async fn answer(mut subscription: Subscription, bell: Doorbell) {
    while let Ok(received) = subscription.recv().await {
        match received {
            // A payload that is not a fleet id was not sent by `publish`, and
            // wakes nobody.
            Received::Message(message) => {
                if Uuid7::parse(&message.payload).is_ok() {
                    bell.ring();
                }
            }
            Received::Lagged(_) | Received::Gap => {
                producers::fleet::ready_wake_failed(ReadyWake::Gap);
            }
        }
    }
}

/// Ring after committing the admission or gate answer. A failure is logged,
/// never returned as a rejection of work already accepted.
pub async fn publish(queue: &Dragonfly, fleet: &str) {
    let shard = crc16::State::<crc16::XMODEM>::calculate(fleet.as_bytes()) % WAKE_SHARDS;
    let streams = FleetStreams::new(queue.clone());
    let sent =
        tokio::time::timeout(READY_WAKE_DEADLINE, streams.publish(&channel(shard), fleet)).await;
    let failure = match sent {
        Ok(Ok(_)) => return,
        Ok(Err(failure)) => failure,
        Err(_) => crate::error::timed_out(crate::streams::CMD_PUBLISH, READY_WAKE_DEADLINE),
    };
    producers::fleet::ready_wake_failed(ReadyWake::Ring);
    let Logged { error_code, reason } = failure.logged();
    tracing::warn!(
        error_code,
        reason,
        fleet_id = fleet,
        shard,
        event = EVENT_RING_FAILED,
        "the notification was lost; the next Postgres probe still finds the admission"
    );
}

fn channel(shard: u16) -> String {
    format!("{CHANNEL_PREFIX}:{{{shard}}}")
}

#[cfg(test)]
mod tests;
