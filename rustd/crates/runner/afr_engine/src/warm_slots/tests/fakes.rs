//! The engine and sandbox the warm-slot tests count with.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use af_core::id::Uuid7;
use af_wire::runner::{Isolation, SandboxEngine};
use afr_guest_client::{GuestAgent, Process, ProcessId, Spawn};
use bytes::Bytes;

use crate::engine::{
    Capabilities, Engine, Environment, Granted, Killed, Limits, ParkMode, Parked, Quiescence,
    ResourceFacts, Root, Sandbox, SandboxSpec, SandboxState, Sweep, Swept,
};
use crate::error::Result;
use crate::warm_slots::WarmSlots;
use crate::{EgressPlan, Generation, NetworkKind};

/// What the counting engine states: every cap enforced, nothing parked.
const CAPABILITIES: Capabilities = Capabilities {
    engine: SandboxEngine::Firecracker,
    isolation: Isolation::HardwareVm,
    parks: &[],
    snapshots: false,
    resources: ResourceFacts::ENFORCED,
};

/// A guest agent nothing calls; a warm slot is only handed out, never driven.
#[derive(Debug)]
pub(super) struct Idle;

fn unused<T>() -> af_guest_wire::Result<T> {
    Err(std::io::Error::other("an idle fake").into())
}

#[async_trait::async_trait]
impl GuestAgent for Idle {
    async fn spawn(&self, _spawn: &Spawn) -> af_guest_wire::Result<Process> {
        unused()
    }
    async fn write(&self, _process: ProcessId, _data: Bytes) -> af_guest_wire::Result<()> {
        unused()
    }
    async fn kill(&self, _process: ProcessId) -> af_guest_wire::Result<()> {
        unused()
    }
}

/// A sandbox that keeps its identifier and counts its own destruction.
#[derive(Debug)]
pub(super) struct Named {
    pub(super) panics: bool,
    /// Whether its teardown is refused, leaving it behind.
    pub(super) leaks: bool,
    pub(super) dead: bool,
    id: Uuid7,
    granted: Granted,
    environment: Environment,
    pub(super) destroyed: Arc<AtomicU64>,
    plans: Arc<Plans>,
    agent: Idle,
}

#[async_trait::async_trait]
impl Sandbox for Named {
    async fn kill(&self) -> Killed {
        unreachable!("a warm slot is handed out or destroyed, never killed")
    }

    fn id(&self) -> &Uuid7 {
        &self.id
    }
    fn environment(&self) -> &Environment {
        &self.environment
    }
    fn granted(&self) -> &Granted {
        &self.granted
    }
    fn agent(&self) -> &dyn GuestAgent {
        &self.agent
    }
    fn state(&self) -> SandboxState {
        if self.dead {
            SandboxState::Ended
        } else {
            SandboxState::Running
        }
    }
    async fn quiesce(&self) -> Result<Quiescence> {
        unreachable!("a warm slot is handed out or destroyed, never captured")
    }
    async fn park(self: Box<Self>, _mode: ParkMode) -> Result<Box<dyn Parked>> {
        unreachable!("a warm slot is handed out or destroyed, never parked")
    }
    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        if self.plans.refused {
            return Err(crate::error::refused("nftables"));
        }
        self.plans.held.lock().unwrap().push(plan.generation);
        Ok(())
    }
    fn snapshots(&self) -> Option<&dyn crate::Snapshots> {
        None
    }
    async fn destroy(self: Box<Self>) -> Result<()> {
        assert!(!self.panics, "a teardown that panics");
        self.destroyed.fetch_add(1, Ordering::SeqCst);
        if self.leaks {
            return Err(crate::error::refused("cgroup"));
        }
        Ok(())
    }
}

/// An engine that builds each sandbox under its spec's identifier and counts
/// them.
#[derive(Debug, Default)]
pub(super) struct Counting {
    pub(super) started: AtomicU64,
    pub(super) destroyed: Arc<AtomicU64>,
    /// How many starts to refuse before one succeeds.
    pub(super) refusals: AtomicU64,
    /// Whether each sandbox it starts has already died.
    pub(super) dead: bool,
    /// How long each start takes once counted.
    pub(super) delay: Duration,
    /// Whether each sandbox it starts fails loudly when destroyed.
    pub(super) panics: bool,
    /// Whether each sandbox it starts refuses its teardown.
    pub(super) leaks: bool,
    /// The admission bound it reports, if any.
    pub(super) bound: Option<crate::AdmissionBound>,
    /// The kind of network each start was built for, in order.
    pub(super) built: Mutex<Vec<NetworkKind>>,
    /// The plans each sandbox it starts is held to after its build.
    pub(super) plans: Arc<Plans>,
}

/// The plans the sandboxes of one engine were held to after their builds.
#[derive(Debug, Default)]
pub(super) struct Plans {
    /// Each plan held, by number.
    pub(super) held: Mutex<Vec<Generation>>,
    /// Whether every sandbox refuses every plan.
    pub(super) refused: bool,
}

#[async_trait::async_trait]
impl Engine for Counting {
    fn capabilities(&self) -> Capabilities {
        CAPABILITIES
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        CAPABILITIES.grant(limits)
    }

    fn admission_bound(&self) -> Option<crate::AdmissionBound> {
        self.bound
    }

    async fn sweep(&self, _scope: Sweep<'_>) -> Result<Swept> {
        unreachable!("warm slots never sweep")
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        let refused = self
            .refusals
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            });
        if refused.is_ok() {
            return Err(crate::error::refused("landlock"));
        }
        self.started.fetch_add(1, Ordering::SeqCst);
        self.built.lock().unwrap().push(spec.egress.network.kind());
        // A zero sleep still waits for the timer, which the yields in
        // `settle` never turn; only a delayed start sleeps at all.
        if !self.delay.is_zero() {
            tokio::time::sleep(self.delay).await;
        }
        Ok(Box::new(Named {
            id: spec.id.clone(),
            granted: spec.granted,
            environment: Environment {
                engine: SandboxEngine::Firecracker,
                root: Root::Host,
            },
            destroyed: Arc::clone(&self.destroyed),
            panics: self.panics,
            leaks: self.leaks,
            dead: self.dead,
            plans: Arc::clone(&self.plans),
            agent: Idle,
        }))
    }
}

/// A pool of `slots` over `inner`, assigned the isolated network.
pub(super) fn isolated(inner: &Arc<Counting>, slots: usize) -> WarmSlots {
    let pool = WarmSlots::start(
        Arc::clone(inner) as Arc<dyn Engine>,
        slots,
        Limits::default(),
    );
    pool.network_assigned(NetworkKind::Isolated);
    pool
}

pub(super) async fn settle() {
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
}
