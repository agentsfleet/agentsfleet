//! The task that owns every held sandbox, and the one way a hold ends.

use std::sync::Arc;

use afd_core::clock::{Clock, UnixMillis};
use afd_core::id::Uuid7;
use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afr_sandbox::Sandbox;
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::record;
use tokio::sync::{Notify, mpsc};
use tokio::task::JoinSet;

use super::{HoldKey, Release, Request, Taken};

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
    since: UnixMillis,
    until: UnixMillis,
}

/// Every hold, how many workers the runner has, and how many are busy.
#[derive(Debug)]
pub(super) struct Keeper {
    clock: Arc<dyn Clock>,
    held: Vec<Entry>,
    workers: usize,
    busy: usize,
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
            busy: 0,
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
                    let until = self.park(key, lease, sandbox);
                    let _unasked = reply.send(until);
                }
                Request::Release { fleet, reason } => self.release(&fleet, reason),
                Request::Supersede { lease } => self.supersede(&lease),
                Request::Discard {
                    fleet,
                    sandbox,
                    reason,
                } => self.destroy(&fleet, sandbox, reason),
                Request::Occupy { fleet } => self.occupy(&fleet),
                Request::Vacate => self.busy = self.busy.saturating_sub(1),
                Request::Resize { workers } => {
                    self.workers = workers;
                    self.cap(workers);
                }
                Request::Fleets { reply } => {
                    let fleets = self.held.iter().map(|entry| entry.key.fleet.clone());
                    let _unasked = reply.send(fleets.collect());
                }
                Request::Shutdown { done } => {
                    self.close().await;
                    let _unasked = done.send(());
                    return;
                }
            }
        }
        self.close().await;
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
        let held_ms = self.clock.now().as_millis() - entry.since.as_millis();
        record::sandbox_hold(SandboxHold::Reused);
        Some(Taken {
            sandbox: entry.sandbox,
            held_ms,
        })
    }

    /// Holds `sandbox` for its fleet until the idle window lapses, after
    /// making room: an older hold of the same fleet goes, then the oldest
    /// until one fewer than the cap remain.
    fn park(
        &mut self,
        key: HoldKey,
        lease: Uuid7,
        sandbox: Box<dyn Sandbox>,
    ) -> Option<UnixMillis> {
        if let Some(at) = self.position(&key.fleet) {
            let stale = self.held.remove(at);
            self.retire(stale, Release::Superseded);
        }
        let Some(room) = self.workers.checked_sub(1) else {
            self.destroy(&key.fleet, sandbox, Release::Capped);
            return None;
        };
        self.cap(room);
        let since = self.clock.now();
        let until = since.saturating_add_millis(SANDBOX_HOLD_IDLE_MS);
        let lease_id = lease.as_str();
        let fleet_id = key.fleet.as_str();
        let deadline_ms = until.as_millis();
        let event = EVENT_HELD;
        tracing::info!(lease_id, fleet_id, deadline_ms, event);
        record::sandbox_hold(SandboxHold::Parked);
        self.held.push(Entry {
            key,
            lease,
            sandbox,
            since,
            until,
        });
        Some(until)
    }

    /// Counts a busy worker; the one that leaves none free releases every
    /// hold but its own fleet's, and says so at once.
    fn occupy(&mut self, fleet: &Uuid7) {
        self.busy += 1;
        if self.busy < self.workers {
            return;
        }
        let before = self.held.len();
        self.retire_where(|entry| entry.key.fleet != *fleet, Release::Saturated);
        if self.held.len() < before {
            self.saturated.notify_one();
        }
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

    fn expire(&mut self) {
        let now = self.clock.now();
        self.retire_where(|entry| entry.until <= now, Release::Expired);
    }

    /// Releases the oldest holds until at most `keep` remain.
    fn cap(&mut self, keep: usize) {
        let over = self.held.len().saturating_sub(keep);
        for entry in self.held.drain(..over).collect::<Vec<_>>() {
            self.retire(entry, Release::Capped);
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
        let fleet = fleet.clone();
        self.destroying
            .spawn(async move { tear_down(&fleet, sandbox).await });
    }

    fn position(&self, fleet: &Uuid7) -> Option<usize> {
        self.held.iter().position(|entry| entry.key.fleet == *fleet)
    }

    async fn close(&mut self) {
        self.retire_where(|_every| true, Release::Shutdown);
        while self.destroying.join_next().await.is_some() {}
    }
}

/// Releases a hold the task can no longer take, because the runner stopped.
pub(super) async fn destroy_now(fleet: &Uuid7, sandbox: Box<dyn Sandbox>, reason: Release) {
    released(fleet, reason);
    tear_down(fleet, sandbox).await;
}

fn released(fleet: &Uuid7, reason: Release) {
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
        tracing::warn!(
            error_code,
            fleet_id,
            event,
            "a released hold's sandbox did not tear down cleanly"
        );
    }
}

#[cfg(test)]
#[path = "keeper_tests.rs"]
mod tests;
