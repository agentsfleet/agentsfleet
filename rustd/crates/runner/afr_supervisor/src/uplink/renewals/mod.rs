//! One host tick snapshots live meters without retaining map guards across work.

use std::sync::Arc;

use af_core::{error_code::ErrorCode, id::Uuid7};
use af_wire::{
    protocol::{RUNS_MAX, Renewal},
    report::RenewResponse,
};
use afr_agent::Meter;
use dashmap::{DashMap, mapref::entry::Entry};
use futures_util::{StreamExt as _, stream::FuturesUnordered};
use parking_lot::Mutex;
use tokio::{
    sync::{OwnedSemaphorePermit, Semaphore, watch},
    time::{Instant, MissedTickBehavior},
};
use tokio_util::sync::CancellationToken;

use super::StreamHandle;
use crate::{Result, client::stream::Outcome, error::ErrorKind};

mod batches;

#[derive(Clone, Debug)]
pub(crate) enum Renewed {
    Confirmed(RenewResponse),
    Refused(ErrorCode),
}

enum Duty {
    Fleet {
        meter: Arc<Meter>,
        updates: watch::Sender<Option<Renewed>>,
    },
    Authoring,
}

struct Run {
    deadline: Arc<Mutex<Instant>>,
    duty: Duty,
}

pub(crate) struct Renewals {
    runs: DashMap<Uuid7, Arc<Run>>,
    slots: Arc<Semaphore>,
}

impl Default for Renewals {
    fn default() -> Self {
        Self {
            runs: DashMap::new(),
            slots: Arc::new(Semaphore::new(RUNS_MAX)),
        }
    }
}

impl std::fmt::Debug for Renewals {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Renewals")
            .field("runs", &self.runs.len())
            .finish_non_exhaustive()
    }
}

impl Renewals {
    pub(crate) fn register(
        self: &Arc<Self>,
        run: &Uuid7,
        meter: Arc<Meter>,
        deadline: Instant,
    ) -> Result<Registration> {
        let (updates, watching) = watch::channel(None);
        let lifetime = self.insert(run, deadline, Duty::Fleet { meter, updates })?;
        Ok(Registration {
            lifetime,
            updates: watching,
        })
    }

    pub(crate) fn authoring(self: &Arc<Self>, run: &Uuid7, deadline: Instant) -> Result<Lifetime> {
        self.insert(run, deadline, Duty::Authoring)
    }

    fn insert(self: &Arc<Self>, run: &Uuid7, deadline: Instant, duty: Duty) -> Result<Lifetime> {
        let slot = Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(crate::error::stream_protocol)?;
        let deadline = Arc::new(Mutex::new(deadline));
        match self.runs.entry(run.clone()) {
            Entry::Occupied(_) => return Err(ErrorKind::StreamProtocol.into()),
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(Run {
                    deadline: Arc::clone(&deadline),
                    duty,
                }));
            }
        }
        Ok(Lifetime {
            registry: Arc::clone(self),
            run: run.clone(),
            deadline,
            _slot: slot,
        })
    }

    /// Renewal owners and recovery read the same confirmed monotonic deadline.
    pub(crate) fn recovery_deadline(&self, budget: Instant) -> Instant {
        let deadlines: Vec<_> = self
            .runs
            .iter()
            .map(|entry| Arc::clone(&entry.deadline))
            .collect();
        deadlines
            .into_iter()
            .map(|deadline| *deadline.lock())
            .min()
            .map_or(budget, |deadline| budget.min(deadline))
    }

    #[cfg(test)]
    pub(crate) async fn run(&self, handle: &StreamHandle, stop: CancellationToken) -> Result<()> {
        self.run_with_interval(handle, stop, af_core::timing::RENEWAL_TICK)
            .await
    }

    pub(crate) async fn run_with_interval(
        &self,
        handle: &StreamHandle,
        stop: CancellationToken,
        interval: std::time::Duration,
    ) -> Result<()> {
        let mut ticks = tokio::time::interval_at(Instant::now() + interval, interval);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut sessions = handle.session.clone();
        let mut previous = sessions.borrow_and_update().clone();
        loop {
            tokio::select! {
                biased;
                () = stop.cancelled() => return Ok(()),
                changed = sessions.changed() => {
                    if changed.is_err() { return Ok(()); }
                    let current = sessions.borrow_and_update().clone();
                    let unchanged = current.as_ref().zip(previous.as_ref())
                        .is_some_and(|(current, previous)| Arc::ptr_eq(current, previous));
                    let connected = current.as_ref().is_some_and(|session| !session.stop.is_cancelled());
                    previous = current;
                    if unchanged { continue; }
                    if !connected { continue; }
                    // Recovery must not spend another tick waiting to renew live work.
                    ticks.reset();
                }
                _ = ticks.tick() => {}
            }
            let renewed = tokio::select! {
                () = stop.cancelled() => return Ok(()),
                result = self.batch(handle) => result,
            };
            match renewed {
                Ok(()) => {}
                Err(error) if error.is_retryable() => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "renew_failed_retry", error_code);
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn batch(&self, handle: &StreamHandle) -> Result<()> {
        let deadline = Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
        let mut active = FuturesUnordered::new();
        for batch in batches::split(self.snapshot())? {
            active.push(handle.compute_until(batch, deadline));
        }
        // At most RUNS_MAX partitions share the existing queue and request bounds.
        // Concurrent answers preserve one tick's budget and each run's own cursor.
        while let Some(answer) = active.next().await {
            self.record(answer?);
        }
        Ok(())
    }

    fn snapshot(&self) -> Vec<(Uuid7, Renewal<'static>)> {
        // Clone identities and handles first. Meter locks never nest under a map shard.
        let runs: Vec<_> = self
            .runs
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        runs.into_iter()
            .filter_map(|(id, run)| {
                let Duty::Fleet { meter, .. } = &run.duty else {
                    return None;
                };
                let request = af_wire::report::RenewRequest {
                    requests: meter.unrecorded(),
                };
                let renewal = Renewal {
                    run_id: id.to_string().into(),
                    request,
                };
                Some((id, renewal))
            })
            .collect()
    }

    fn record(&self, runs: Vec<(Uuid7, Outcome<RenewResponse>)>) {
        for (id, answer) in runs {
            let run = self.runs.get(&id).map(|entry| Arc::clone(entry.value()));
            let Some(run) = run else {
                continue;
            };
            let Duty::Fleet { updates, .. } = &run.duty else {
                continue;
            };
            // Releasing the shard before watch notification permits concurrent retirements.
            match answer {
                Outcome::Accepted(response) => {
                    updates.send_replace(Some(Renewed::Confirmed(response)));
                }
                Outcome::Refused {
                    code,
                    retryable: false,
                } => {
                    updates.send_replace(Some(Renewed::Refused(code)));
                }
                Outcome::Refused {
                    code,
                    retryable: true,
                } => {
                    let error_code = code.as_str();
                    tracing::debug!(event = "runner_stream_renewal_retry", error_code);
                }
            }
        }
    }
}

pub(crate) struct Registration {
    pub(crate) lifetime: Lifetime,
    pub(crate) updates: watch::Receiver<Option<Renewed>>,
}

/// One live run's authoritative deadline, released when renewal ownership ends.
pub(crate) struct Lifetime {
    registry: Arc<Renewals>,
    run: Uuid7,
    deadline: Arc<Mutex<Instant>>,
    _slot: OwnedSemaphorePermit,
}

impl Lifetime {
    pub(crate) fn deadline(&self) -> Instant {
        *self.deadline.lock()
    }
    pub(crate) fn extend(&self, deadline: Instant) {
        *self.deadline.lock() = deadline;
    }
}

impl Drop for Lifetime {
    fn drop(&mut self) {
        self.registry.runs.remove(&self.run);
    }
}

#[cfg(test)]
mod tests;
