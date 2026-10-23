//! Sharded notifications prioritize recent work; Postgres remains authoritative.
//! The bounded local hints may be dropped at any time without losing admissions.

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
    collections::VecDeque,
    sync::{Arc, Mutex},
};

/// Independent publish channels spread notifications over cluster slots.
pub const WAKE_SHARDS: u16 = 16;
const HINT_CAPACITY: usize = 64;
const CHANNEL_PREFIX: &str = "fleet:wake";
const EVENT_RING_FAILED: &str = "admission_wake_ring_failed";

/// Recent fleet hints shared by the hub listener and every lease poll.
#[derive(Debug, Clone, Default)]
pub struct Doorbell {
    hints: Arc<Mutex<VecDeque<Uuid7>>>,
}

impl Doorbell {
    /// An empty hint buffer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store a hint without allowing a burst to grow process memory.
    pub fn ring(&self, fleet: Uuid7) {
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
