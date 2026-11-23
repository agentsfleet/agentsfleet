//! Sandboxes held between a fleet's runs, so its next run continues where
//! the last one stopped instead of in an empty workspace.
//!
//! A run that ends processed parks its sandbox here, frozen, for the fleet's
//! next run: the files it wrote, the processes it left running and whatever
//! it cached all stay. One task owns every hold and a run reaches it through
//! a channel, the shape `afr_sandbox::WarmSlots` uses, so no lock guards the
//! holds and a sandbox moves by value from the run that used it to the one
//! that takes it.
//!
//! A hold ends when its idle window runs out, when the runner's last free
//! worker would be consumed, when fewer workers leave room for it, when the daemon names its fleet, when it no longer
//! carries the fleet's latest run ([`Release::Superseded`]), and once the
//! runner takes no new run. Expiry is checked on every message, and the
//! heartbeat asks for the list every tick, so no timer runs here and a test
//! moves time through the clock alone.

use std::sync::Arc;
use std::time::Duration;

use af_core::clock::Clock;
use af_core::id::Uuid7;
use afr_sandbox::{Limits, Sandbox};
use afr_telemetry::labels::SandboxHold;
use jiff::Timestamp;
use tokio::sync::{Notify, mpsc, oneshot};

mod built_under;
mod keeper;
mod occupancy;

pub(crate) use occupancy::Occupant;

pub(crate) use self::built_under::BuiltUnder;
use self::keeper::Keeper;

/// What a held sandbox must match to serve a run. A run that differs in
/// any of these gets a fresh sandbox, and the hold is destroyed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HoldKey {
    /// The fleet it served.
    pub(crate) fleet: Uuid7,
    /// The workspace that fleet belongs to.
    pub(crate) workspace: String,
    /// The size it enforces.
    pub(crate) limits: Limits,
    /// The network policy and repository binding it was built under, so a
    /// changed policy never runs in a sandbox built for the old one.
    pub(crate) policy: BuiltUnder,
}

/// Why a hold ended without its fleet's next run taking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Release {
    /// Its idle window ran out.
    Expired,
    /// The runner's last free worker took a run for another fleet.
    Saturated,
    /// The runner held as many as it has workers, and this was the oldest.
    Capped,
    /// The next run wanted another size or policy.
    Mismatch,
    /// The daemon named its fleet: halted, deleted, or held by another
    /// runner since.
    Inactive,
    /// The runner takes no new run, so no run could take it: running
    /// stopped, or the runner is shutting down or stopped.
    Shutdown,
    /// It would not take its next run's addresses, would not thaw, or its
    /// executor did not answer once thawed.
    ThawFailed,
    /// It no longer carries the fleet's latest run: the daemon refused the
    /// report of the run that left it, as settled without it or for good,
    /// or never received it; the fleet's next run was not told to resume
    /// it; a newer park of the same fleet replaced it; or the run that
    /// asked for it stopped waiting before taking it.
    Superseded,
}

impl Release {
    /// The label the runner's holds family counts it under.
    pub(crate) const fn outcome(self) -> SandboxHold {
        match self {
            Self::Expired => SandboxHold::Expired,
            Self::Saturated => SandboxHold::Saturated,
            Self::Capped => SandboxHold::Capped,
            Self::Mismatch => SandboxHold::Mismatch,
            Self::Inactive => SandboxHold::Inactive,
            Self::Shutdown => SandboxHold::Shutdown,
            Self::ThawFailed => SandboxHold::ThawFailed,
            Self::Superseded => SandboxHold::Superseded,
        }
    }
}

/// A sandbox its fleet's next run took, still frozen, and how long it was
/// held.
#[derive(Debug)]
pub(crate) struct Taken {
    /// The sandbox, for the taker to thaw.
    pub(crate) sandbox: Box<dyn Sandbox>,
    /// How long it waited.
    pub(crate) held: Duration,
}

/// What a run asks the task that owns the holds.
#[derive(Debug)]
enum Request {
    Take {
        key: HoldKey,
        reply: oneshot::Sender<Option<Taken>>,
    },
    Park {
        key: HoldKey,
        run: Uuid7,
        sandbox: Box<dyn Sandbox>,
        reply: oneshot::Sender<Parked>,
    },
    Release {
        fleet: Uuid7,
        reason: Release,
    },
    Supersede {
        run: Uuid7,
    },
    Accepted {
        run: Uuid7,
        until: Timestamp,
    },
    Discard {
        fleet: Uuid7,
        sandbox: Box<dyn Sandbox>,
        reason: Release,
    },
    Occupy {
        occupant: Occupant,
    },
    Vacate {
        run: Uuid7,
    },
    Resize {
        workers: usize,
    },
    Fleets {
        reply: oneshot::Sender<Vec<Uuid7>>,
    },
    Close,
    Shutdown {
        done: oneshot::Sender<()>,
    },
}

/// The runner's held sandboxes, shared by every worker.
#[derive(Debug, Clone)]
pub(crate) struct Holds {
    requests: mpsc::UnboundedSender<Request>,
    saturated: Arc<Notify>,
}

/// What a park answers: the hold's lapse, or the sandbox back.
#[derive(Debug)]
#[must_use]
pub(crate) enum Parked {
    /// Held for the fleet's next run until then.
    Held(Timestamp),
    /// Not held: the keeper is closed or has no room. The release is
    /// already counted; the sandbox, still frozen, is the run's to end.
    Refused(Box<dyn Sandbox>),
    /// The keeper stopped with the request, and the sandbox went with it.
    Gone,
}

impl Holds {
    /// Starts the task that owns every hold, timing each by `clock`. It holds
    /// nothing until [`Holds::resize`] says how many workers the runner has.
    pub(crate) fn start(clock: Arc<dyn Clock>) -> Self {
        let (requests, received) = mpsc::unbounded_channel();
        let saturated = Arc::new(Notify::new());
        tokio::spawn(Keeper::new(clock, Arc::clone(&saturated)).run(received));
        Self {
            requests,
            saturated,
        }
    }

    /// [`Holds::start`], counting into `recorder` what the task counts: it is
    /// spawned, so a test's own recording scope never reaches it.
    #[cfg(test)]
    pub(crate) fn recording(
        clock: Arc<dyn Clock>,
        recorder: Arc<dyn afr_telemetry::Recorder>,
    ) -> Self {
        let (requests, received) = mpsc::unbounded_channel();
        let saturated = Arc::new(Notify::new());
        let keeper = Keeper::new(clock, Arc::clone(&saturated)).run(received);
        tokio::spawn(afr_telemetry::testing::scoped(recorder, keeper));
        Self {
            requests,
            saturated,
        }
    }

    /// Ends every hold and holds nothing more, for a runner that takes no new
    /// run: no run could take one. A later park hands its sandbox back to
    /// its run, which kills it and releases it, and the next list asked for
    /// is empty.
    pub(crate) fn close(&self) {
        let _stopped = self.requests.send(Request::Close);
    }

    /// The fleet's held sandbox when `key` matches it whole, still frozen. A
    /// hold that differs is destroyed, and the run builds a fresh one.
    pub(crate) async fn take(&self, key: &HoldKey) -> Option<Taken> {
        let (reply, answer) = oneshot::channel();
        let key = key.clone();
        self.requests.send(Request::Take { key, reply }).ok()?;
        answer.await.ok().flatten()
    }

    /// Holds `sandbox`, already frozen, for `key`'s fleet, and answers when
    /// the hold lapses. A sandbox the keeper will not hold comes back, its
    /// release logged and counted, for the run to kill now and release
    /// after its report.
    pub(crate) async fn park(&self, key: HoldKey, run: Uuid7, sandbox: Box<dyn Sandbox>) -> Parked {
        let (reply, answer) = oneshot::channel();
        let parked = Request::Park {
            key,
            run,
            sandbox,
            reply,
        };
        // A request the stopped task never took comes back whole, and so
        // does its sandbox.
        if let Err(closed) = self.requests.send(parked)
            && let Request::Park { key, sandbox, .. } = closed.0
        {
            keeper::released(&key.fleet, Release::Shutdown);
            return Parked::Refused(sandbox);
        }
        answer.await.unwrap_or(Parked::Gone)
    }

    /// Ends `fleet`'s hold, if the runner has one, for `reason`.
    pub(crate) fn release(&self, fleet: Uuid7, reason: Release) {
        let _stopped = self.requests.send(Request::Release { fleet, reason });
    }

    /// Ends the hold `run` parked, if it is still held, because the daemon
    /// settled `run` without its report. A later run's hold of the same
    /// fleet stays: that run took the sandbox and parked it again.
    pub(crate) fn supersede(&self, run: Uuid7) {
        let _stopped = self.requests.send(Request::Supersede { run });
    }

    /// Narrows the exact parked attempt to the daemon's committed deadline.
    pub(crate) fn accept(&self, run: Uuid7, until: Timestamp) {
        let _stopped = self.requests.send(Request::Accepted { run, until });
    }

    /// Destroys `sandbox`, which was `fleet`'s hold, for `reason`: logged and
    /// counted as every release is.
    pub(crate) async fn discard(&self, fleet: Uuid7, sandbox: Box<dyn Sandbox>, reason: Release) {
        let discarded = Request::Discard {
            fleet,
            sandbox,
            reason,
        };
        if let Err(closed) = self.requests.send(discarded)
            && let Request::Discard { fleet, sandbox, .. } = closed.0
        {
            keeper::destroy_now(&fleet, sandbox, reason).await;
        }
    }

    /// Sets how many workers the runner has, preserving one free worker
    /// after busy workers and held sandboxes; the oldest past that go.
    pub(crate) fn resize(&self, workers: usize) {
        let _stopped = self.requests.send(Request::Resize { workers });
    }

    /// Every fleet the runner holds a sandbox for, after expiring the lapsed.
    pub(crate) async fn fleets(&self) -> Vec<Uuid7> {
        let (reply, answer) = oneshot::channel();
        if self.requests.send(Request::Fleets { reply }).is_err() {
            return Vec::new();
        }
        answer.await.unwrap_or_default()
    }

    /// Rung when holds were released because the runner ran out of free
    /// workers, so the daemon hears at once rather than at the next tick.
    pub(crate) fn saturated(&self) -> &Notify {
        &self.saturated
    }

    /// Destroys every hold and waits until each is gone.
    pub(crate) async fn shutdown(&self) {
        let (done, finished) = oneshot::channel();
        if self.requests.send(Request::Shutdown { done }).is_ok() {
            let _gone = finished.await;
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod supersede_tests;

#[cfg(test)]
mod shutdown_tests;

#[cfg(test)]
mod transfer_tests;
