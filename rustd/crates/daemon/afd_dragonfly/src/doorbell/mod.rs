//! Sharded notifications prioritize recent work; Postgres remains authoritative.
//! The bounded local hints may be dropped at any time without losing admissions.

use crate::{
    Dragonfly, FleetStreams, SubscriptionHub,
    hub::{Received, Subscription},
};
use af_core::{
    error_code::{Coded as _, Logged},
    id::Uuid7,
    timing::READY_WAKE_DEADLINE,
};
use af_observability::{metrics::label::fleet::ReadyWake, producers};
use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::watch;

/// Independent publish channels spread notifications over cluster slots.
pub const WAKE_SHARDS: u16 = 16;
const HINT_CAPACITY: usize = 64;
const CHANNEL_PREFIX: &str = "fleet:wake";
const EVENT_RING_FAILED: &str = "admission_wake_ring_failed";

/// Recent fleet hints shared by the hub listener and every lease poll, and the
/// rings a held heartbeat waits for.
#[derive(Debug, Clone, Default)]
pub struct Doorbell {
    hints: Arc<Mutex<VecDeque<Uuid7>>>,
    rings: Arc<Rings>,
}

/// Every ring this process heard, as a generation a held beat compares
/// against, and how many beats are held waiting for the next one.
///
/// A `watch` rather than a `Notify`: a holder takes the generation when it
/// arrives and waits for it to move, so a ring that lands between the two is
/// still seen, and every holder hears every ring.
#[derive(Debug)]
struct Rings {
    generation: watch::Sender<u64>,
    held: AtomicUsize,
    /// Set once the process stops accepting requests: no beat is held after.
    closed: AtomicBool,
}

impl Default for Rings {
    fn default() -> Self {
        Self {
            generation: watch::Sender::new(0),
            held: AtomicUsize::new(0),
            closed: AtomicBool::new(false),
        }
    }
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
    /// An empty hint buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a hint without allowing a burst to grow process memory.
    ///
    /// The generation moves first, before the dedup: a second admission for a
    /// fleet whose hint is still buffered is fresh work a held beat must hear.
    pub fn ring(&self, fleet: Uuid7) {
        self.rings
            .generation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
        let mut hints = self
            .hints
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if hints.contains(&fleet) {
            return;
        }
        if hints.len() == HINT_CAPACITY {
            hints.pop_front();
        }
        hints.push_back(fleet);
    }

    /// Consume one hint. A missing hint still requires the Postgres probe.
    #[must_use]
    pub fn take(&self) -> Option<Uuid7> {
        self.hints
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    /// Holds the caller until the next ring or `deadline`, unless `max` beats
    /// are already held. The generation is taken before the wait starts, so a
    /// ring between the two is not lost.
    pub async fn wait_for_ring(&self, deadline: Duration, max: usize) -> Waited {
        // Subscribed before the closed check: a close between the two moves
        // the generation this receiver already watches, so no wait sleeps
        // through it.
        let mut generation = self.rings.generation.subscribe();
        if self.is_closed() {
            return Waited::Deadline;
        }
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
        match tokio::time::timeout(deadline, generation.changed()).await {
            Ok(Ok(())) if self.is_closed() => Waited::Deadline,
            Ok(Ok(())) => Waited::Rung,
            // The sender lives as long as this doorbell, so a closed channel
            // cannot happen while `self` is borrowed; it reads as no ring.
            Ok(Err(_)) | Err(_) => Waited::Deadline,
        }
    }

    /// Ends every held wait at once and holds no more, for a process that has
    /// stopped accepting requests: a held beat would otherwise keep its drain
    /// waiting until its deadline. The waits answer as at their deadline.
    pub fn close(&self) {
        self.rings.closed.store(true, Ordering::Release);
        self.rings
            .generation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
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
            Received::Message(message) => {
                if let Ok(fleet) = Uuid7::parse(&message.payload) {
                    bell.ring(fleet);
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
