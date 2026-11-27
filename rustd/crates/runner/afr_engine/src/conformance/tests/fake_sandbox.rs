//! The fake engine's sandbox, its snapshots, and the sandbox parked.

use af_core::id::Uuid7;
use afr_guest_client::GuestAgent;

use super::{ENGINE, FakeGuest, Flaw};
use crate::engine::{
    EgressPlan, Environment, Granted, Killed, ParkMode, Parked, Quiescence, Sandbox, SandboxState,
    SnapshotRef, Snapshots,
};
use crate::error::{Result, unsupported};

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

    async fn egress(&mut self, _plan: EgressPlan<'_>) -> Result<()> {
        Ok(())
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

#[async_trait::async_trait]
impl Snapshots for FakeSandbox {
    async fn take(&self, id: Uuid7) -> Result<SnapshotRef> {
        let snapshot = SnapshotRef::new(id);
        let state = &mut *self.agent.state();
        state.saved.push((snapshot.clone(), state.disk.clone()));
        Ok(snapshot)
    }

    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()> {
        let flaw = self.agent.flaw;
        let state = &mut *self.agent.state();
        let mut disk = state
            .saved
            .iter()
            .find(|(taken, _disk)| taken == snapshot)
            .map(|(_taken, disk)| disk.clone())
            .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::NotFound))?;
        if flaw == Flaw::RestoreChangesAByte
            && let Some(byte) = disk.files.values_mut().flatten().next()
        {
            *byte ^= 1;
        }
        state.disk = disk;
        state.running &= flaw == Flaw::RestoreKeepsAProcess;
        Ok(())
    }

    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.agent
            .state()
            .saved
            .retain(|(taken, _disk)| taken != snapshot);
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
