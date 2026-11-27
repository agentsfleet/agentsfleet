//! A microVM's snapshots: its workspace as files in the chunk store its build
//! was handed, restored inside the running VM.
//!
//! ```text
//!   take(id)       freeze tenant ─► pin walk ─► thaw ─► publish
//!   restore(ref)   environment? ─► tree ─► renew ─► import ─► hosts ─► plan read back
//! ```
//!
//! The monitor keeps running through both: a restore ends every process of
//! the guest's inner environment and builds it again, and returns only once
//! the run's egress plan reads back in force, so a snapshot taken under an
//! older plan never brings it back.

use af_core::id::Uuid7;
use af_wire::runner::SandboxEngine;
use afr_engine::error::unsupported;
use afr_engine::{FileSnapshots, Result, SnapshotRef, Snapshots, until_in_force};

use super::{Reach, Vm, resolve};

/// What a sandbox built with no chunk store is asked, and refuses.
const SNAPSHOT: &str = "snapshot";

impl Vm {
    /// Its snapshots, or the refusal of a sandbox built with no store.
    fn files(&self) -> Result<&FileSnapshots> {
        self.kept
            .snapshots
            .as_ref()
            .ok_or_else(|| unsupported(SandboxEngine::Firecracker, SNAPSHOT))
    }
}

#[async_trait::async_trait]
impl Snapshots for Vm {
    async fn take(&self, id: Uuid7) -> Result<SnapshotRef> {
        self.files()?
            .take(&self.client, &self.kept.environment, id)
            .await
    }

    /// Restores the workspace inside the running VM, then gives the renewed
    /// environment the run's hosts file again and reads the run's plan back
    /// in force before the guest agent serves anything.
    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.files()?
            .restore(&self.client, &self.kept.environment, snapshot)
            .await?;
        let plan = &self.kept.plan;
        if let Reach::Allowed(allowlist) = &plan.reach {
            resolve(&self.client, allowlist).await?;
        }
        let within = self.shared.config.tuning.ready_timeout;
        until_in_force(self, plan.generation, within).await
    }

    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.files()?.forget(snapshot).await
    }
}
