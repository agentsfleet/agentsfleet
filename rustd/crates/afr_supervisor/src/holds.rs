//! Sandboxes held between a fleet's leases, so its next lease continues where
//! the last one stopped instead of in an empty workspace.
//!
//! A lease that ends processed parks its sandbox here, frozen, for the fleet's
//! next lease: the files it wrote, the processes it left running and whatever
//! it cached all stay. One task owns every hold and a lease reaches it through
//! a channel, the shape `afr_sandbox::WarmSlots` uses, so no lock guards the
//! holds and a sandbox moves by value from the lease that used it to the one
//! that takes it.
//!
//! A hold ends when its idle window runs out, when the runner's last free
//! worker takes a lease for another fleet (a runner with no worker free could
//! not serve it anyway), when the runner holds as many as it has workers and
//! it is the oldest, when the daemon names its fleet, when it no longer
//! carries the fleet's latest run ([`Release::Superseded`]), and once the
//! runner takes no new lease. Expiry is checked on every message, and the
//! heartbeat asks for the list every tick, so no timer runs here and a test
//! moves time through the clock alone.

use std::sync::Arc;

use afd_core::clock::{Clock, UnixMillis};
use afd_core::id::Uuid7;
use afr_sandbox::{Limits, Sandbox};
use afr_telemetry::labels::SandboxHold;
use tokio::sync::{Notify, mpsc, oneshot};

mod built_under;
mod keeper;

pub(crate) use self::built_under::BuiltUnder;
use self::keeper::Keeper;

/// What a held sandbox must match to serve a lease. A lease that differs in
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

/// Why a hold ended without its fleet's next lease taking it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Release {
    /// Its idle window ran out.
    Expired,
    /// The runner's last free worker took a lease for another fleet.
    Saturated,
    /// The runner held as many as it has workers, and this was the oldest.
    Capped,
    /// The next lease wanted another size or policy.
    Mismatch,
    /// The daemon named its fleet: halted, deleted, or leased by another
    /// runner since.
    Inactive,
    /// The runner takes no new lease, so no lease could take it: leasing
    /// stopped, or the runner is shutting down or stopped.
    Shutdown,
    /// It would not take its next lease's addresses, would not thaw, or its
    /// executor did not answer once thawed.
    ThawFailed,
    /// It no longer carries the fleet's latest run: the daemon refused the
    /// report of the lease that left it, as settled without it or for good,
    /// or never received it; the fleet's next lease was not told to resume
    /// it; a newer park of the same fleet replaced it; or the lease that
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

/// A sandbox its fleet's next lease took, still frozen, and how long it was
/// held.
#[derive(Debug)]
pub(crate) struct Taken {
    /// The sandbox, for the taker to thaw.
    pub(crate) sandbox: Box<dyn Sandbox>,
    /// How long it waited, in milliseconds.
    pub(crate) held_ms: i64,
}

/// What a lease asks the task that owns the holds.
#[derive(Debug)]
enum Request {
    Take {
        key: HoldKey,
        reply: oneshot::Sender<Option<Taken>>,
    },
    Park {
        key: HoldKey,
        lease: Uuid7,
        sandbox: Box<dyn Sandbox>,
        reply: oneshot::Sender<Option<UnixMillis>>,
    },
    Release {
        fleet: Uuid7,
        reason: Release,
    },
    Supersede {
        lease: Uuid7,
    },
    Discard {
        fleet: Uuid7,
        sandbox: Box<dyn Sandbox>,
        reason: Release,
    },
    Occupy {
        fleet: Uuid7,
    },
    Vacate,
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
    /// lease: no lease could take one. A later park destroys its sandbox, and
    /// the next list asked for is empty.
    pub(crate) fn close(&self) {
        let _stopped = self.requests.send(Request::Close);
    }

    /// The fleet's held sandbox when `key` matches it whole, still frozen. A
    /// hold that differs is destroyed, and the lease builds a fresh one.
    pub(crate) async fn take(&self, key: &HoldKey) -> Option<Taken> {
        let (reply, answer) = oneshot::channel();
        let key = key.clone();
        self.requests.send(Request::Take { key, reply }).ok()?;
        answer.await.ok().flatten()
    }

    /// Holds `sandbox`, already frozen, for `key`'s fleet, and answers when
    /// the hold lapses; `None` when it is not held, and was destroyed.
    pub(crate) async fn park(
        &self,
        key: HoldKey,
        lease: Uuid7,
        sandbox: Box<dyn Sandbox>,
    ) -> Option<UnixMillis> {
        let (reply, answer) = oneshot::channel();
        let parked = Request::Park {
            key,
            lease,
            sandbox,
            reply,
        };
        if let Err(closed) = self.requests.send(parked) {
            if let Request::Park { key, sandbox, .. } = closed.0 {
                keeper::destroy_now(&key.fleet, sandbox, Release::Shutdown).await;
            }
            return None;
        }
        answer.await.ok().flatten()
    }

    /// Ends `fleet`'s hold, if the runner has one, for `reason`.
    pub(crate) fn release(&self, fleet: Uuid7, reason: Release) {
        let _stopped = self.requests.send(Request::Release { fleet, reason });
    }

    /// Ends the hold `lease` parked, if it is still held, because the daemon
    /// settled `lease` without its report. A later lease's hold of the same
    /// fleet stays: that lease took the sandbox and parked it again.
    pub(crate) fn supersede(&self, lease: Uuid7) {
        let _stopped = self.requests.send(Request::Supersede { lease });
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

    /// Counts a worker busy with a lease for `fleet` until the guard drops.
    /// The worker that leaves none free releases every other fleet's hold.
    pub(crate) fn occupy(&self, fleet: Uuid7) -> Occupied {
        let _stopped = self.requests.send(Request::Occupy { fleet });
        Occupied(self.requests.clone())
    }

    /// Sets how many workers the runner has, which is also how many sandboxes
    /// it may hold; the oldest past that go.
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

/// A worker busy with a lease; dropping it frees the worker.
#[derive(Debug)]
pub(crate) struct Occupied(mpsc::UnboundedSender<Request>);

impl Drop for Occupied {
    fn drop(&mut self) {
        let _stopped = self.0.send(Request::Vacate);
    }
}

#[cfg(test)]
#[path = "holds/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "holds/supersede_tests.rs"]
mod supersede_tests;
