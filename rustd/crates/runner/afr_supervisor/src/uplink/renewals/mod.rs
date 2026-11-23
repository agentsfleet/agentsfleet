//! The runner's one clock: each round wakes the host heartbeat and renews the runs held,
//! snapshotting live meters without retaining map guards across work.

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
use std::time::Duration;

use tokio::{
    sync::{Notify, OwnedSemaphorePermit, Semaphore, watch},
    time::Instant,
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

struct Run {
    deadline: Arc<Mutex<Instant>>,
    meter: Arc<Meter>,
    updates: watch::Sender<Option<Renewed>>,
}

pub(crate) struct Renewals {
    runs: DashMap<Uuid7, Arc<Run>>,
    slots: Arc<Semaphore>,
    /// Each round's wake for the heartbeat loop: one stored permit, so a round that lands
    /// while a beat is in flight or backing off is kept for its next turn, and a session
    /// with no heartbeat loop loses nothing.
    beat: Notify,
    /// The interval the daemon last advertised on a heartbeat answer.
    interval: watch::Sender<Duration>,
}

impl Default for Renewals {
    fn default() -> Self {
        Self {
            runs: DashMap::new(),
            slots: Arc::new(Semaphore::new(RUNS_MAX)),
            beat: Notify::new(),
            interval: watch::Sender::new(af_core::timing::HEARTBEAT_INTERVAL),
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
        let lifetime = self.insert(run, deadline, meter, updates)?;
        Ok(Registration {
            lifetime,
            updates: watching,
        })
    }

    fn insert(
        self: &Arc<Self>,
        run: &Uuid7,
        deadline: Instant,
        meter: Arc<Meter>,
        updates: watch::Sender<Option<Renewed>>,
    ) -> Result<Lifetime> {
        let slot = Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(crate::error::stream_protocol)?;
        let deadline = Arc::new(Mutex::new(deadline));
        match self.runs.entry(run.clone()) {
            Entry::Occupied(_) => return Err(ErrorKind::StreamProtocol.into()),
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(Run {
                    deadline: Arc::clone(&deadline),
                    meter,
                    updates,
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

    /// The wake a round leaves for the heartbeat loop.
    pub(crate) const fn beats(&self) -> &Notify {
        &self.beat
    }

    /// Takes the interval a heartbeat answer advertised for the rounds after it.
    pub(crate) fn advertised(&self, interval: Duration) {
        self.interval.send_replace(interval);
    }

    fn interval(&self) -> Duration {
        *self.interval.borrow()
    }

    /// The runner's one clock. A round is due one heartbeat interval after the previous
    /// round was sent, and never before that round's answers are in. Each round wakes the
    /// heartbeat loop and renews the runs held. A round that failed, or in which a run was
    /// refused for now, is followed by another one answer bound after it was sent, or one
    /// interval when that is shorter. A new connected session starts a round at once.
    pub(crate) async fn run(&self, handle: &StreamHandle, stop: CancellationToken) -> Result<()> {
        let mut sessions = handle.session.clone();
        let mut previous = sessions.borrow_and_update().clone();
        let mut due = Instant::now() + self.interval();
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
                    if unchanged || !connected { continue; }
                    // Recovery must not spend another round waiting to renew live work.
                }
                () = tokio::time::sleep_until(due) => {}
            }
            let sent = Instant::now();
            self.beat.notify_one();
            let renewed = tokio::select! {
                () = stop.cancelled() => return Ok(()),
                result = self.batch(handle) => result,
            };
            let interval = self.interval();
            let retry = interval.min(af_core::timing::STREAM_ANSWER_TIMEOUT);
            due = match renewed {
                Ok(Settled::All) => sent + interval,
                Ok(Settled::RetryLater) => sent + retry,
                Err(error) if error.is_retryable() => {
                    let error_code = error.code().as_str();
                    tracing::warn!(event = "renew_failed_retry", error_code);
                    sent + retry
                }
                Err(error) => return Err(error),
            };
        }
    }

    async fn batch(&self, handle: &StreamHandle) -> Result<Settled> {
        let deadline = Instant::now() + af_core::timing::STREAM_ANSWER_TIMEOUT;
        let mut active = FuturesUnordered::new();
        for batch in batches::split(self.snapshot())? {
            active.push(handle.compute_until(batch, deadline));
        }
        // At most RUNS_MAX partitions share the existing queue and request bounds.
        // Concurrent answers preserve one tick's budget and each run's own cursor.
        let mut settled = Settled::All;
        while let Some(answer) = active.next().await {
            if self.record(answer?) {
                settled = Settled::RetryLater;
            }
        }
        Ok(settled)
    }

    fn snapshot(&self) -> Vec<(Uuid7, Renewal<'static>)> {
        // Clone identities and handles first. Meter locks never nest under a map shard.
        let runs: Vec<_> = self
            .runs
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        runs.into_iter()
            .map(|(id, run)| {
                let request = af_wire::report::RenewRequest {
                    requests: run.meter.unrecorded(),
                };
                let renewal = Renewal {
                    run_id: id.to_string().into(),
                    request,
                };
                (id, renewal)
            })
            .collect()
    }

    /// Hands each run its answer; whether any run was refused for now.
    fn record(&self, runs: Vec<(Uuid7, Outcome<RenewResponse>)>) -> bool {
        let mut retry = false;
        for (id, answer) in runs {
            let run = self.runs.get(&id).map(|entry| Arc::clone(entry.value()));
            let Some(run) = run else {
                continue;
            };
            let updates = &run.updates;
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
                    retry = true;
                }
            }
        }
        retry
    }
}

/// How a round's renewals ended.
enum Settled {
    /// Every run held was answered for good.
    All,
    /// A run was refused for now, so the next round comes sooner.
    RetryLater,
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
