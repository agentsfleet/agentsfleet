//! Bounded recent-work hints; a missing hint is recovered by the replica probe.

mod config;
mod hints;
mod publish;

use std::sync::{Arc, Mutex};

use af_core::id::Uuid7;
use af_observability::{metrics::label::fleet::ReadyWake, producers};
use tokio::{sync::watch, task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{Handle, Received, Result, Subject};

pub use config::{DEFAULT_HINT_CAPACITY, HINT_CAPACITY_ENV, MAX_HINT_CAPACITY};
pub use publish::publish;

/// Independent subjects spread hints across the broker's shard slots.
pub const SHARDS: u16 = 16;

/// One hint buffer and generation counter per replica, shared by its sessions.
#[derive(Debug, Clone)]
pub struct Wake {
    hints: Arc<Mutex<hints::Hints>>,
    generation: watch::Sender<u64>,
}

impl Default for Wake {
    fn default() -> Self {
        Self::bounded(DEFAULT_HINT_CAPACITY)
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
    /// Refuses zero or more than [`MAX_HINT_CAPACITY`] entries before allocation.
    pub fn with_capacity(capacity: usize) -> Result<Self> {
        Ok(Self::bounded(config::prove(capacity)?))
    }

    fn bounded(capacity: usize) -> Self {
        Self {
            hints: Arc::new(Mutex::new(hints::Hints::new(capacity))),
            generation: watch::Sender::new(0),
        }
    }

    /// Publish the hint before notifying sessions; duplicate admissions still notify.
    pub fn ring(&self, fleet: Uuid7) {
        let outcome = self.lock().ring(fleet);
        producers::fleet::runner::stream::hint(outcome);
        self.generation
            .send_modify(|generation| *generation = generation.wrapping_add(1));
    }

    /// Consume one hint; absence never proves that Postgres has no ready work.
    #[must_use]
    pub fn take(&self) -> Option<Uuid7> {
        self.lock().take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, hints::Hints> {
        match self.hints.lock() {
            Ok(guard) => guard,
            // Hints are disposable; Postgres owns the work even after a worker panic.
            Err(poisoned) => {
                let mut hints = poisoned.into_inner();
                hints.clear();
                self.hints.clear_poison();
                producers::fleet::ready_wake_failed(ReadyWake::Gap);
                let error_code = af_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(event = "admission_wake_hints_recovered", error_code);
                hints
            }
        }
    }

    /// Observe generations before probing so a concurrent ring cannot be missed.
    #[must_use]
    pub fn generations(&self) -> watch::Receiver<u64> {
        self.generation.subscribe()
    }

    /// Own all shard listeners in the returned task set; dropping it aborts them.
    ///
    /// # Errors
    /// Returns a failed subscription and drops any listeners already opened.
    pub async fn listen(&self, bus: &Handle, stop: &CancellationToken) -> Result<JoinSet<()>> {
        let mut workers = JoinSet::new();
        for shard in 0..SHARDS {
            let subscription = bus.subscribe(&Subject::Wake(shard)).await?;
            workers.spawn(receive(self.clone(), subscription, stop.clone()));
        }
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
                        producers::fleet::ready_wake_failed(ReadyWake::Gap);
                        let error_code = error.code().as_str();
                        tracing::warn!(event = "admission_wake_invalid_hint", error_code);
                    }
                },
                Ok(Received::Lagged(_) | Received::Gap) => producers::fleet::ready_wake_failed(ReadyWake::Gap),
                Err(error) => {
                    producers::fleet::ready_wake_failed(ReadyWake::Gap);
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "admission_wake_subscription_closed", error_code);
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
