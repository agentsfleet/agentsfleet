//! Bounded recent-work queue; a missed notification is recovered by the replica probe.

mod config;
mod publish;
mod queue;

use std::sync::{Arc, Mutex};

use af_core::id::Uuid7;
use af_observability::{metrics::label::fleet::ReadyWake, producers};
use tokio::{sync::watch, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{Handle, Received, Result};

pub use config::{DEFAULT_WAKE_QUEUE_CAPACITY, ENV_WAKE_QUEUE_CAPACITY, MAX_WAKE_QUEUE_CAPACITY};
pub use publish::publish;

/// Identity of an independent recovery request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recovery(u64);

/// Ordinary notification count and independent recovery work observed together.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Generation {
    rings: u64,
    recovery_sequence: u64,
    recovery_pending: bool,
}

impl Generation {
    /// Ordinary rings grant an equal number of successful offers.
    #[must_use]
    pub const fn rings(self) -> u64 {
        self.rings
    }

    /// Recovery requests start a bounded drain independently of ring count.
    #[must_use]
    pub const fn recovery(self) -> Option<Recovery> {
        if self.recovery_pending {
            Some(Recovery(self.recovery_sequence))
        } else {
            None
        }
    }
}

/// One wake queue and generation counter per replica, shared by its sessions.
#[derive(Debug, Clone)]
pub struct Wake {
    queue: Arc<Mutex<queue::WakeQueue>>,
    generation: watch::Sender<Generation>,
}

impl Default for Wake {
    fn default() -> Self {
        Self::bounded(DEFAULT_WAKE_QUEUE_CAPACITY)
    }
}

impl Wake {
    /// Resolve the capacity once during daemon boot, before accepting sessions.
    ///
    /// # Errors
    /// Refuses malformed, zero or excessive capacities without allocating the queue.
    pub fn from_env(env: &(impl af_core::env::EnvSource + ?Sized)) -> Result<Self> {
        Ok(Self::bounded(config::resolve(env)?))
    }

    /// Construct an isolated replica queue for an explicitly configured capacity.
    ///
    /// # Errors
    /// Refuses zero or more than [`MAX_WAKE_QUEUE_CAPACITY`] entries before allocation.
    pub fn with_capacity(capacity: usize) -> Result<Self> {
        Ok(Self::bounded(config::prove(capacity)?))
    }

    fn bounded(capacity: usize) -> Self {
        Self {
            queue: Arc::new(Mutex::new(queue::WakeQueue::new(capacity))),
            generation: watch::Sender::new(Generation::default()),
        }
    }

    /// Enqueue the fleet before notifying sessions; duplicate admissions still notify.
    pub fn ring(&self, fleet: Uuid7) {
        let outcome = self.lock().ring(fleet);
        producers::fleet::runner::stream::wake_queue(outcome);
        self.generation
            .send_modify(|generation| generation.rings = generation.rings.wrapping_add(1));
    }

    /// An unknown loss wakes a full eligible-session pass without fabricating a fleet identifier.
    pub fn recheck(&self) {
        self.generation.send_modify(|generation| {
            generation.recovery_sequence = generation.recovery_sequence.wrapping_add(1);
            generation.recovery_pending = true;
        });
    }

    /// A recovery request survives empty hosts and ordinary locality notifications.
    #[must_use]
    pub fn recovery(&self) -> Option<Recovery> {
        self.generation.borrow().recovery()
    }

    /// A finished drain covers only the recovery request that started it.
    pub fn recovered(&self, recovery: Recovery) {
        self.generation.send_if_modified(|generation| {
            if generation.recovery() == Some(recovery) {
                generation.recovery_pending = false;
                true
            } else {
                false
            }
        });
    }

    /// Consume one fleet; absence never proves that Postgres has no ready work.
    #[must_use]
    pub fn take(&self) -> Option<Uuid7> {
        self.lock().take()
    }

    /// Scheduling reads locality preferences without consuming the claim's reminder.
    #[must_use]
    pub fn fleets(&self) -> std::collections::HashSet<Uuid7> {
        self.lock().fleets()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, queue::WakeQueue> {
        match self.queue.lock() {
            Ok(guard) => guard,
            // Notifications are disposable; Postgres owns the work even after a worker panic.
            Err(poisoned) => {
                let mut queue = poisoned.into_inner();
                queue.clear();
                self.queue.clear_poison();
                producers::fleet::ready_wake_failed(ReadyWake::Gap);
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(event = "turn_wake_queue_recovered", error_code);
                queue
            }
        }
    }

    /// Observe generations before probing so a concurrent ring cannot be missed.
    #[must_use]
    pub fn generations(&self) -> watch::Receiver<Generation> {
        self.generation.subscribe()
    }

    /// Own the one listener over every thread's wake in the returned task
    /// set; dropping it aborts the listener.
    ///
    /// # Errors
    /// Returns a failed subscription.
    pub async fn listen(&self, bus: &Handle, stop: &CancellationToken) -> Result<JoinSet<()>> {
        let subscription = bus.subscribe_thread_wakes().await?;
        let mut workers = JoinSet::new();
        workers.spawn(receive(self.clone(), subscription, stop.clone()));
        Ok(workers)
    }
}

async fn receive(wake: Wake, mut subscription: crate::Subscription, stop: CancellationToken) {
    loop {
        tokio::select! {
            biased;
            () = stop.cancelled() => return,
            next = subscription.recv() => match next {
                Ok(Received::Message(message)) => match Uuid7::parse(&message.payload) {
                    Ok(fleet) => wake.ring(fleet),
                    Err(error) => {
                        wake.recheck();
                        producers::fleet::ready_wake_failed(ReadyWake::Gap);
                        let error_code = error.code().as_str();
                        tracing::warn!(event = "turn_wake_invalid_notification", error_code);
                    }
                },
                Ok(Received::Lagged(_) | Received::Gap) => {
                    wake.recheck();
                    producers::fleet::ready_wake_failed(ReadyWake::Gap);
                }
                Err(error) => {
                    wake.recheck();
                    producers::fleet::ready_wake_failed(ReadyWake::Gap);
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "turn_wake_subscription_closed", error_code);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod listen_tests;
#[cfg(test)]
mod tests;
