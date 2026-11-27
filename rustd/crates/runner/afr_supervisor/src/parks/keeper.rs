//! The task that owns every parked sandbox, and the one way a park ends.

use std::{collections::BTreeMap, sync::Arc};

use af_core::clock::{self, Clock};
use af_core::id::Uuid7;
use afr_engine::Parked;
use afr_telemetry::labels::SandboxPark;
use afr_telemetry::record;
use jiff::Timestamp;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinSet;

use super::{ParkKey, Release, Request, Stowed, Taken};

/// The event a parked sandbox is logged under.
const EVENT_PARKED: &str = "sandbox_parked";
/// The event a park that ended without reuse is logged under.
const EVENT_RELEASED: &str = "sandbox_park_released";
/// The event a released park that did not tear down is logged under.
const EVENT_DESTROY_FAILED: &str = "sandbox_park_destroy_failed";

/// One parked sandbox, oldest first in [`Keeper::parked`].
#[derive(Debug)]
struct Entry {
    key: ParkKey,
    /// The run that parked it, which alone may end it as superseded.
    run: Uuid7,
    sandbox: Box<dyn Parked>,
    since: Timestamp,
    until: Timestamp,
}

/// Every park, how many workers the runner has, and how many are busy.
#[derive(Debug)]
pub(super) struct Keeper {
    clock: Arc<dyn Clock>,
    parked: Vec<Entry>,
    workers: usize,
    busy: BTreeMap<Uuid7, Uuid7>,
    /// Set once the runner takes no new run; nothing is parked after.
    closed: bool,
    /// Every teardown in flight, so shutdown can wait for each one.
    destroying: JoinSet<()>,
    saturated: Arc<Notify>,
}

impl Keeper {
    pub(super) fn new(clock: Arc<dyn Clock>, saturated: Arc<Notify>) -> Self {
        Self {
            clock,
            parked: Vec::new(),
            workers: 0,
            busy: BTreeMap::new(),
            closed: false,
            destroying: JoinSet::new(),
            saturated,
        }
    }

    /// Answers requests until the runner shuts down or every handle is gone,
    /// then destroys every park and waits for each teardown.
    pub(super) async fn run(mut self, mut requests: mpsc::UnboundedReceiver<Request>) {
        while let Some(request) = requests.recv().await {
            while self.destroying.try_join_next().is_some() {}
            self.expire();
            match request {
                Request::Take { key, reply } => {
                    if let Err(Some(unclaimed)) = reply.send(self.take(&key)) {
                        self.destroy(&key.fleet, unclaimed.sandbox, Release::Superseded);
                    }
                }
                Request::Park {
                    key,
                    run,
                    sandbox,
                    reply,
                } => {
                    let fleet = key.fleet.clone();
                    match reply.send(self.park(key, run, sandbox)) {
                        // Nobody waits for a refused sandbox: its release is
                        // already counted, so it is only torn down here.
                        Err(Stowed::Refused(sandbox)) => {
                            self.destroying
                                .spawn(async move { tear_down(&fleet, sandbox).await });
                        }
                        // Parked for a run that stopped waiting: no report will
                        // name the park, so it is superseded now, not at its
                        // lapse.
                        Err(Stowed::Parked(_)) => self.release(&fleet, Release::Superseded),
                        Ok(()) | Err(Stowed::Gone) => {}
                    }
                }
                Request::Release { fleet, reason } => self.release(&fleet, reason),
                Request::Supersede { run } => self.supersede(&run),
                Request::Accepted { run, until } => self.accept(&run, until),
                Request::Occupy { occupant } => {
                    self.busy.insert(occupant.run, occupant.fleet);
                    self.cap(self.room(), Release::Saturated);
                }
                Request::Vacate { run } => {
                    self.busy.remove(&run);
                    self.cap(self.room(), Release::Capped);
                }
                Request::Resize { workers } => {
                    self.workers = workers;
                    self.cap(self.room(), Release::Capped);
                }
                Request::Fleets { reply } => {
                    let fleets = self.parked.iter().map(|entry| entry.key.fleet.clone());
                    let _unasked = reply.send(fleets.collect());
                }
                Request::Close => self.close(),
                Request::Shutdown { done } => {
                    self.finish().await;
                    let _unasked = done.send(());
                    return;
                }
            }
        }
        self.finish().await;
    }

    /// Hands out `key.fleet`'s park when the rest of `key` matches it, and
    /// destroys it when it does not.
    fn take(&mut self, key: &ParkKey) -> Option<Taken> {
        let at = self.position(&key.fleet)?;
        let entry = self.parked.remove(at);
        if entry.key != *key {
            self.retire(entry, Release::Mismatch);
            return None;
        }
        let parked = clock::elapsed(entry.since, self.clock.now());
        Some(Taken {
            sandbox: entry.sandbox,
            parked,
        })
    }

    /// Parks `sandbox` for its fleet until the idle window lapses, after
    /// making room: an older park of the same fleet goes, then the oldest
    /// until one fewer than the cap remain. A closed keeper parks nothing.
    fn park(&mut self, key: ParkKey, run: Uuid7, sandbox: Box<dyn Parked>) -> Stowed {
        if self.closed {
            released(&key.fleet, Release::Shutdown);
            return Stowed::Refused(sandbox);
        }
        if let Some(at) = self.position(&key.fleet) {
            let stale = self.parked.remove(at);
            self.retire(stale, Release::Superseded);
        }
        // A successful park consumes this run's busy reservation atomically.
        // Its guard may still be alive while the report awaits acknowledgement.
        let transfers = self.busy.get(&run) == Some(&key.fleet);
        let busy = self.busy.len() - usize::from(transfers);
        let room = self.workers.saturating_sub(busy).saturating_sub(1);
        if room == 0 {
            released(&key.fleet, Release::Capped);
            return Stowed::Refused(sandbox);
        }
        if transfers {
            self.busy.remove(&run);
        }
        self.cap(room - 1, Release::Capped);
        let since = self.clock.now();
        // The park asks for the most a daemon may grant, the ceiling of its
        // `max_idle_park_seconds`; the daemon commits its configured length and the
        // report's acknowledgement narrows the park to it, so the setting is the one
        // source of how long a sandbox stays parked.
        let until = clock::after(since, af_core::timing::DAY);
        let run_id = run.as_str();
        let fleet_id = key.fleet.as_str();
        let event = EVENT_PARKED;
        tracing::info!(run_id, fleet_id, deadline = %until, event);
        record::sandbox_park(SandboxPark::Parked);
        self.parked.push(Entry {
            key,
            run,
            sandbox,
            since,
            until,
        });
        Stowed::Parked(until)
    }

    fn room(&self) -> usize {
        self.workers
            .saturating_sub(self.busy.len())
            .saturating_sub(1)
    }

    fn release(&mut self, fleet: &Uuid7, reason: Release) {
        if let Some(at) = self.position(fleet) {
            let entry = self.parked.remove(at);
            self.retire(entry, reason);
        }
    }

    /// Ends the park `run` parked, for a superseded report; a park a later
    /// run parked is that run's.
    fn supersede(&mut self, run: &Uuid7) {
        if let Some(at) = self.parked.iter().position(|entry| entry.run == *run) {
            let entry = self.parked.remove(at);
            self.retire(entry, Release::Superseded);
        }
    }

    fn accept(&mut self, run: &Uuid7, until: Timestamp) {
        if let Some(entry) = self.parked.iter_mut().find(|entry| entry.run == *run)
            && until < entry.until
        {
            entry.until = until;
        }
        self.expire();
    }

    fn expire(&mut self) {
        let now = self.clock.now();
        self.retire_where(|entry| entry.until <= now, Release::Expired);
    }

    /// A park transferring into a busy fleet is already charged by that worker.
    fn cap(&mut self, keep: usize, reason: Release) {
        let busy = &self.busy;
        let protected = |entry: &Entry| busy.values().any(|fleet| *fleet == entry.key.fleet);
        let mut over = self
            .parked
            .iter()
            .filter(|entry| !protected(entry))
            .count()
            .saturating_sub(keep);
        let mut retired = Vec::new();
        let mut kept = Vec::new();
        for entry in std::mem::take(&mut self.parked) {
            if over > 0 && !protected(&entry) {
                over -= 1;
                retired.push(entry);
            } else {
                kept.push(entry);
            }
        }
        self.parked = kept;
        for entry in retired {
            self.retire(entry, reason);
        }
    }

    fn retire_where(&mut self, gone: impl Fn(&Entry) -> bool, reason: Release) {
        let (retired, kept) = std::mem::take(&mut self.parked)
            .into_iter()
            .partition::<Vec<_>, _>(|entry| gone(entry));
        self.parked = kept;
        for entry in retired {
            self.retire(entry, reason);
        }
    }

    fn retire(&mut self, entry: Entry, reason: Release) {
        self.destroy(&entry.key.fleet, entry.sandbox, reason);
    }

    /// Logs and counts the release, then tears the sandbox down off this task.
    fn destroy(&mut self, fleet: &Uuid7, sandbox: Box<dyn Parked>, reason: Release) {
        released(fleet, reason);
        if !self.closed {
            self.saturated.notify_one();
        }
        let fleet = fleet.clone();
        self.destroying
            .spawn(async move { tear_down(&fleet, sandbox).await });
    }

    fn position(&self, fleet: &Uuid7) -> Option<usize> {
        self.parked
            .iter()
            .position(|entry| entry.key.fleet == *fleet)
    }

    /// Ends every park, and parks nothing from here on.
    fn close(&mut self) {
        self.closed = true;
        self.retire_where(|_every| true, Release::Shutdown);
    }

    /// Ends every park and waits for each teardown in flight.
    async fn finish(&mut self) {
        self.close();
        while self.destroying.join_next().await.is_some() {}
    }
}

pub(super) fn released(fleet: &Uuid7, reason: Release) {
    let fleet_id = fleet.as_str();
    let outcome = reason.outcome();
    let reason = outcome.as_str();
    let event = EVENT_RELEASED;
    tracing::info!(fleet_id, reason, event);
    record::sandbox_park(outcome);
}

async fn tear_down(fleet: &Uuid7, sandbox: Box<dyn Parked>) {
    if let Err(failure) = sandbox.destroy().await {
        let error_code = failure.code().as_str();
        let fleet_id = fleet.as_str();
        let event = EVENT_DESTROY_FAILED;
        {
            let (message,) = (format_args!(
                "a released park's sandbox did not tear down cleanly"
            ),);
            tracing::warn!(message, error_code, fleet_id, event);
        };
    }
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;
