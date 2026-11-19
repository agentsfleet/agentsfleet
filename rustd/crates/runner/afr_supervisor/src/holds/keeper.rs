//! The task that owns every held sandbox, and the one way a hold ends.

use std::{collections::BTreeMap, sync::Arc};

use af_core::clock::{self, Clock};
use af_core::id::Uuid7;
use af_core::timing::SANDBOX_HOLD_IDLE;
use afr_sandbox::Sandbox;
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::record;
use jiff::Timestamp;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinSet;

use super::{HoldKey, Parked, Release, Request, Taken};

/// The event a parked sandbox is logged under.
const EVENT_HELD: &str = "sandbox_held";
/// The event a hold that ended without reuse is logged under.
const EVENT_RELEASED: &str = "sandbox_hold_released";
/// The event a released hold that did not tear down is logged under.
const EVENT_DESTROY_FAILED: &str = "sandbox_hold_destroy_failed";

/// One held sandbox, oldest first in [`Keeper::held`].
#[derive(Debug)]
struct Entry {
    key: HoldKey,
    /// The lease that parked it, which alone may end it as superseded.
    lease: Uuid7,
    sandbox: Box<dyn Sandbox>,
    since: Timestamp,
    until: Timestamp,
}

/// Every hold, how many workers the runner has, and how many are busy.
#[derive(Debug)]
pub(super) struct Keeper {
    clock: Arc<dyn Clock>,
    held: Vec<Entry>,
    workers: usize,
    busy: BTreeMap<Uuid7, Option<Uuid7>>,
    /// Set once the runner takes no new lease; nothing is held after.
    closed: bool,
    /// Every teardown in flight, so shutdown can wait for each one.
    destroying: JoinSet<()>,
    saturated: Arc<Notify>,
}

impl Keeper {
    pub(super) fn new(clock: Arc<dyn Clock>, saturated: Arc<Notify>) -> Self {
        Self {
            clock,
            held: Vec::new(),
            workers: 0,
            busy: BTreeMap::new(),
            closed: false,
            destroying: JoinSet::new(),
            saturated,
        }
    }

    /// Answers requests until the runner shuts down or every handle is gone,
    /// then destroys every hold and waits for each teardown.
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
                    lease,
                    sandbox,
                    reply,
                } => {
                    let fleet = key.fleet.clone();
                    match reply.send(self.park(key, lease, sandbox)) {
                        // Nobody waits for a refused sandbox: its release is
                        // already counted, so it is only torn down here.
                        Err(Parked::Refused(sandbox)) => {
                            self.destroying
                                .spawn(async move { tear_down(&fleet, sandbox).await });
                        }
                        // Held for a lease that stopped waiting: no report will
                        // name the hold, so it is superseded now, not at its
                        // lapse.
                        Err(Parked::Held(_)) => self.release(&fleet, Release::Superseded),
                        Ok(()) | Err(Parked::Gone) => {}
                    }
                }
                Request::Release { fleet, reason } => self.release(&fleet, reason),
                Request::Supersede { lease } => self.supersede(&lease),
                Request::Accepted { lease, until } => self.accept(&lease, until),
                Request::Discard {
                    fleet,
                    sandbox,
                    reason,
                } => self.destroy(&fleet, sandbox, reason),
                Request::Occupy { occupant } => {
                    let (lease, fleet) = occupant.parts();
                    self.busy.insert(lease, fleet);
                    self.cap(self.room(), Release::Saturated);
                }
                Request::Vacate { lease } => {
                    self.busy.remove(&lease);
                    self.cap(self.room(), Release::Capped);
                }
                Request::Resize { workers } => {
                    self.workers = workers;
                    self.cap(self.room(), Release::Capped);
                }
                Request::Fleets { reply } => {
                    let fleets = self.held.iter().map(|entry| entry.key.fleet.clone());
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

    /// Hands out `key.fleet`'s hold when the rest of `key` matches it, and
    /// destroys it when it does not.
    fn take(&mut self, key: &HoldKey) -> Option<Taken> {
        let at = self.position(&key.fleet)?;
        let entry = self.held.remove(at);
        if entry.key != *key {
            self.retire(entry, Release::Mismatch);
            return None;
        }
        let held = af_core::clock::elapsed(entry.since, self.clock.now());
        Some(Taken {
            sandbox: entry.sandbox,
            held,
        })
    }

    /// Holds `sandbox` for its fleet until the idle window lapses, after
    /// making room: an older hold of the same fleet goes, then the oldest
    /// until one fewer than the cap remain. A closed keeper holds nothing.
    fn park(&mut self, key: HoldKey, lease: Uuid7, sandbox: Box<dyn Sandbox>) -> Parked {
        if self.closed {
            released(&key.fleet, Release::Shutdown);
            return Parked::Refused(sandbox);
        }
        if let Some(at) = self.position(&key.fleet) {
            let stale = self.held.remove(at);
            self.retire(stale, Release::Superseded);
        }
        // A successful park consumes this lease's busy reservation atomically.
        // Its guard may still be alive while the report awaits acknowledgement.
        let transfers = self.busy.get(&lease).and_then(Option::as_ref) == Some(&key.fleet);
        let busy = self.busy.len() - usize::from(transfers);
        let room = self.workers.saturating_sub(busy).saturating_sub(1);
        if room == 0 {
            released(&key.fleet, Release::Capped);
            return Parked::Refused(sandbox);
        }
        if transfers {
            self.busy.remove(&lease);
        }
        self.cap(room - 1, Release::Capped);
        let since = self.clock.now();
        let until = clock::after(since, SANDBOX_HOLD_IDLE);
        let lease_id = lease.as_str();
        let fleet_id = key.fleet.as_str();
        let event = EVENT_HELD;
        tracing::info!(lease_id, fleet_id, deadline = %until, event);
        record::sandbox_hold(SandboxHold::Parked);
        self.held.push(Entry {
            key,
            lease,
            sandbox,
            since,
            until,
        });
        Parked::Held(until)
    }

    fn room(&self) -> usize {
        self.workers
            .saturating_sub(self.busy.len())
            .saturating_sub(1)
    }

    fn release(&mut self, fleet: &Uuid7, reason: Release) {
        if let Some(at) = self.position(fleet) {
            let entry = self.held.remove(at);
            self.retire(entry, reason);
        }
    }

    /// Ends the hold `lease` parked, for a superseded report; a hold a later
    /// lease parked is that lease's.
    fn supersede(&mut self, lease: &Uuid7) {
        if let Some(at) = self.held.iter().position(|entry| entry.lease == *lease) {
            let entry = self.held.remove(at);
            self.retire(entry, Release::Superseded);
        }
    }

    fn accept(&mut self, lease: &Uuid7, until: Timestamp) {
        if let Some(entry) = self.held.iter_mut().find(|entry| entry.lease == *lease)
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

    /// A hold transferring into a busy fleet is already charged by that worker.
    fn cap(&mut self, keep: usize, reason: Release) {
        let busy = &self.busy;
        let protected = |entry: &Entry| {
            busy.values()
                .any(|fleet| fleet.as_ref() == Some(&entry.key.fleet))
        };
        let mut over = self
            .held
            .iter()
            .filter(|entry| !protected(entry))
            .count()
            .saturating_sub(keep);
        let mut retired = Vec::new();
        let mut kept = Vec::new();
        for entry in std::mem::take(&mut self.held) {
            if over > 0 && !protected(&entry) {
                over -= 1;
                retired.push(entry);
            } else {
                kept.push(entry);
            }
        }
        self.held = kept;
        for entry in retired {
            self.retire(entry, reason);
        }
    }

    fn retire_where(&mut self, gone: impl Fn(&Entry) -> bool, reason: Release) {
        let (retired, kept) = std::mem::take(&mut self.held)
            .into_iter()
            .partition::<Vec<_>, _>(|entry| gone(entry));
        self.held = kept;
        for entry in retired {
            self.retire(entry, reason);
        }
    }

    fn retire(&mut self, entry: Entry, reason: Release) {
        self.destroy(&entry.key.fleet, entry.sandbox, reason);
    }

    /// Logs and counts the release, then tears the sandbox down off this task.
    fn destroy(&mut self, fleet: &Uuid7, sandbox: Box<dyn Sandbox>, reason: Release) {
        released(fleet, reason);
        if !self.closed {
            self.saturated.notify_one();
        }
        let fleet = fleet.clone();
        self.destroying
            .spawn(async move { tear_down(&fleet, sandbox).await });
    }

    fn position(&self, fleet: &Uuid7) -> Option<usize> {
        self.held.iter().position(|entry| entry.key.fleet == *fleet)
    }

    /// Ends every hold, and holds nothing from here on.
    fn close(&mut self) {
        self.closed = true;
        self.retire_where(|_every| true, Release::Shutdown);
    }

    /// Ends every hold and waits for each teardown in flight.
    async fn finish(&mut self) {
        self.close();
        while self.destroying.join_next().await.is_some() {}
    }
}

/// Releases a hold the task can no longer take, because the runner stopped.
pub(super) async fn destroy_now(fleet: &Uuid7, sandbox: Box<dyn Sandbox>, reason: Release) {
    released(fleet, reason);
    tear_down(fleet, sandbox).await;
}

pub(super) fn released(fleet: &Uuid7, reason: Release) {
    let fleet_id = fleet.as_str();
    let outcome = reason.outcome();
    let reason = outcome.as_str();
    let event = EVENT_RELEASED;
    tracing::info!(fleet_id, reason, event);
    record::sandbox_hold(outcome);
}

async fn tear_down(fleet: &Uuid7, sandbox: Box<dyn Sandbox>) {
    if let Err(failure) = sandbox.destroy().await {
        let error_code = failure.code().as_str();
        let fleet_id = fleet.as_str();
        let event = EVENT_DESTROY_FAILED;
        {
            let (message,) = (format_args!(
                "a released hold's sandbox did not tear down cleanly"
            ),);
            tracing::warn!(message, error_code, fleet_id, event);
        };
    }
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;
