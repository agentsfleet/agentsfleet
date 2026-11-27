//! A microVM's snapshots: its workspace quiesced through the guest agent's
//! walk, published by the daemon's pipeline or into the chunk store its build
//! was handed, and restored inside the running VM.
//!
//! ```text
//!   quiesce(parent)  freeze tenant ─► pin walk ─► thaw         the fleet runs on
//!   publish(q, id)   changes ─► store ─► release(q, id)
//!   restore(ref)     environment? ─► tree ─► renew ─► import ─► hosts ─► plan read back
//! ```
//!
//! The monitor keeps running through all of it: a restore ends every process
//! of the guest's inner environment and builds it again, and returns only
//! once the run's egress plan reads back in force, so a snapshot taken under
//! an older plan never brings it back.

use af_core::id::Uuid7;
use afr_engine::{Quiesced, Result, SnapshotRef, Snapshots, until_in_force};

use super::{Reach, Vm, resolve};

#[async_trait::async_trait]
impl Snapshots for Vm {
    async fn quiesce(&self, parent: Option<&Uuid7>) -> Result<Box<dyn Quiesced + '_>> {
        self.kept.snapshots.quiesce(self, parent).await
    }

    async fn publish<'a>(
        &'a self,
        quiesced: Box<dyn Quiesced + 'a>,
        id: Uuid7,
    ) -> Result<SnapshotRef> {
        self.kept.snapshots.publish(self, quiesced, id).await
    }

    async fn take(&self, id: Uuid7) -> Result<SnapshotRef> {
        self.kept.snapshots.take(self, id).await
    }

    /// Restores the workspace inside the running VM, then gives the renewed
    /// environment the run's hosts file again and reads the run's plan back
    /// in force before the guest agent serves anything.
    async fn restore(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.kept.snapshots.restore(self, snapshot).await?;
        let plan = &self.kept.plan;
        if let Reach::Allowed(allowlist) = &plan.reach {
            resolve(&self.client, allowlist).await?;
        }
        let within = self.shared.config.tuning.ready_timeout;
        until_in_force(self, plan.generation, within).await
    }

    async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.kept.snapshots.forget(snapshot).await
    }
}
