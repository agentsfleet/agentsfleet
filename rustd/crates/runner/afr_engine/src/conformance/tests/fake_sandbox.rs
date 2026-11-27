//! The fake engine's sandbox, its plan and its snapshots, and the sandbox
//! parked.

use std::time::Duration;

use af_chunk_store::ChunkSource;
use af_core::id::Uuid7;
use afr_guest_client::GuestAgent;

use super::quiesce::{FakeQuiesced, files};
use super::{ENGINE, FakeGuest, Flaw};
use crate::engine::volume::Quiesced;
use crate::engine::{
    EgressEnforcer, EgressPlan, Environment, Generation, Granted, Killed, ParkMode, Parked,
    Quiescence, Sandbox, SandboxState, SnapshotFormat, SnapshotRef, Snapshots, enforce,
};
use crate::error::{Result, unsupported};

/// How long a fake's plan takes to read back: it holds whatever it is given.
const READ_BACK: Duration = Duration::from_millis(10);

/// A fake sandbox.
#[derive(Debug)]
pub(super) struct FakeSandbox {
    pub(super) id: Uuid7,
    pub(super) granted: Granted,
    pub(super) environment: Environment,
    pub(super) snapshots: bool,
    pub(super) agent: FakeGuest,
}

#[async_trait::async_trait]
impl Sandbox for FakeSandbox {
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
        SandboxState::Running
    }

    async fn quiesce(&self) -> Result<Quiescence> {
        Ok(if self.agent.state().running {
            Quiescence::Busy
        } else {
            Quiescence::Quiet
        })
    }

    async fn park(self: Box<Self>, mode: ParkMode) -> Result<Box<dyn Parked>> {
        if mode == ParkMode::Pause && self.agent.flaw == Flaw::PauseUnsupported {
            return Err(unsupported(ENGINE, "park"));
        }
        Ok(Box::new(FakeParked {
            sandbox: self,
            mode,
        }))
    }

    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        enforce(self, plan, READ_BACK).await
    }

    fn snapshots(&self) -> Option<&dyn Snapshots> {
        self.snapshots.then_some(self as &dyn Snapshots)
    }

    async fn kill(&self) -> Killed {
        self.agent.state().running = false;
        Killed::Settled
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}

impl FakeSandbox {
    /// Keeps `disk` under `id`, numbered after every snapshot before it.
    fn saved(&self, id: Uuid7, disk: super::Disk) -> SnapshotRef {
        let state = &mut *self.agent.state();
        let generation = u64::try_from(state.saved.len()).unwrap_or(u64::MAX) + 1;
        let environment = self.environment.clone();
        let snapshot = SnapshotRef::new(id, environment, generation, SnapshotFormat::Files);
        state.saved.push((snapshot.clone(), disk));
        snapshot
    }
}

#[async_trait::async_trait]
impl EgressEnforcer for FakeSandbox {
    async fn apply(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        self.agent.state().disk.plan = plan.generation;
        Ok(())
    }

    async fn in_force(&self) -> Result<Option<Generation>> {
        Ok(Some(self.agent.state().disk.plan))
    }
}

#[async_trait::async_trait]
impl Snapshots for FakeSandbox {
    async fn quiesce(&self, _parent: Option<&Uuid7>) -> Result<Box<dyn Quiesced + '_>> {
        Ok(Box::new(FakeQuiesced(
            self.agent.state().disk.files.clone(),
        )))
    }

    /// Saves the files the quiesce fixed, beside the tree and plan of now.
    async fn publish<'a>(
        &'a self,
        quiesced: Box<dyn Quiesced + 'a>,
        id: Uuid7,
    ) -> Result<SnapshotRef> {
        let files = files(quiesced.changes()).await?;
        quiesced.release(Some(&id)).await?;
        let mut disk = self.agent.state().disk.clone();
        disk.files = files;
        Ok(self.saved(id, disk))
    }

    async fn take(&self, id: Uuid7) -> Result<SnapshotRef> {
        let disk = self.agent.state().disk.clone();
        Ok(self.saved(id, disk))
    }

    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()> {
        let flaw = self.agent.flaw;
        if flaw != Flaw::RestoreCrossesEnvironments {
            snapshot.restores_on(&self.environment)?;
        }
        let state = &mut *self.agent.state();
        let mut disk = state
            .saved
            .iter()
            .find(|(taken, _disk)| taken.id() == snapshot.id())
            .map(|(_taken, disk)| disk.clone())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        if flaw == Flaw::RestoreChangesAByte
            && let Some(byte) = disk.files.values_mut().flatten().next()
        {
            *byte ^= 1;
        }
        if flaw != Flaw::RestoreBringsBackThePlan {
            disk.plan = state.disk.plan;
        }
        state.disk = disk;
        state.running &= flaw == Flaw::RestoreKeepsAProcess;
        Ok(())
    }

    /// The fake keeps what it saved itself, so a source holds nothing it
    /// reads: the restore is the one its own saves name.
    async fn restore_from(&self, snapshot: &SnapshotRef, _source: &dyn ChunkSource) -> Result<()> {
        self.restore(snapshot).await
    }

    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.agent
            .state()
            .saved
            .retain(|(taken, _disk)| taken.id() != snapshot.id());
        Ok(())
    }
}

/// A fake sandbox parked: a stop ends its process, a pause keeps it.
#[derive(Debug)]
struct FakeParked {
    sandbox: Box<FakeSandbox>,
    mode: ParkMode,
}

#[async_trait::async_trait]
impl Parked for FakeParked {
    fn id(&self) -> &Uuid7 {
        &self.sandbox.id
    }

    fn mode(&self) -> ParkMode {
        self.mode
    }

    async fn resume(self: Box<Self>) -> Result<Box<dyn Sandbox>> {
        if self.mode == ParkMode::Stop {
            self.sandbox.agent.state().running = false;
        }
        Ok(self.sandbox)
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        Ok(())
    }
}
