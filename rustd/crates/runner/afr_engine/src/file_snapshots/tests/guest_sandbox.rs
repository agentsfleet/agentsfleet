//! The fake guest agent as the sandbox around it, and the snapshots it
//! offers when a test hands it some.

use af_chunk_store::ChunkSource;
use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_guest_client::GuestAgent;

use super::{Fault, Guest};
use crate::engine::volume::Quiesced;
use crate::engine::{
    EgressPlan, Environment, Granted, Killed, ParkMode, Parked, Quiescence, Sandbox, SandboxState,
    SnapshotRef, Snapshots,
};
use crate::error::unsupported;
use crate::file_snapshots::FileSnapshots;

#[async_trait::async_trait]
impl Sandbox for Guest {
    fn id(&self) -> &Uuid7 {
        &self.id
    }

    fn environment(&self) -> &Environment {
        static OURS: std::sync::LazyLock<Environment> =
            std::sync::LazyLock::new(super::super::ours);
        &OURS
    }

    fn granted(&self) -> &Granted {
        &self.granted
    }

    fn agent(&self) -> &dyn GuestAgent {
        self
    }

    fn state(&self) -> SandboxState {
        SandboxState::Running
    }

    async fn quiesce(&self) -> crate::Result<Quiescence> {
        Ok(if self.fault == Fault::NoLeafBusy {
            Quiescence::Busy
        } else {
            Quiescence::Quiet
        })
    }

    async fn park(self: Box<Self>, _mode: ParkMode) -> crate::Result<Box<dyn Parked>> {
        Err(unsupported(SandboxEngine::Firecracker, "park"))
    }

    async fn egress(&mut self, _plan: EgressPlan<'_>) -> crate::Result<()> {
        Ok(())
    }

    fn snapshots(&self) -> Option<&dyn Snapshots> {
        self.files.is_some().then_some(self as &dyn Snapshots)
    }

    async fn kill(&self) -> Killed {
        Killed::Settled
    }

    async fn destroy(self: Box<Self>) -> crate::Result<()> {
        Ok(())
    }
}

#[async_trait::async_trait]
impl Snapshots for Guest {
    async fn quiesce(&self, parent: Option<&Uuid7>) -> crate::Result<Box<dyn Quiesced + '_>> {
        self.served().quiesce(self, parent).await
    }

    async fn publish<'a>(
        &'a self,
        quiesced: Box<dyn Quiesced + 'a>,
        id: Uuid7,
    ) -> crate::Result<SnapshotRef> {
        self.served().publish(self, quiesced, id).await
    }

    async fn take(&self, id: Uuid7) -> crate::Result<SnapshotRef> {
        self.served().take(self, id).await
    }

    async fn restore(&self, snapshot: &SnapshotRef) -> crate::Result<()> {
        self.served().restore(self, snapshot).await
    }

    async fn restore_from(
        &self,
        snapshot: &SnapshotRef,
        source: &dyn ChunkSource,
    ) -> crate::Result<()> {
        self.served().restore_from(self, snapshot, source).await
    }

    async fn forget(&self, snapshot: &SnapshotRef) -> crate::Result<()> {
        self.served().forget(snapshot).await
    }
}

impl Guest {
    /// The snapshots [`Sandbox::snapshots`] offered it through.
    fn served(&self) -> &FileSnapshots {
        self.files
            .as_ref()
            .unwrap_or_else(|| unreachable!("offered only with snapshots"))
    }
}
