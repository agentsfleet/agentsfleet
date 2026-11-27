//! An unconfined engine whose workspaces a recovery drill can checkpoint
//! (M-TEST-UTIL).
//!
//! A workspace checkpoint names the sandbox's verified toolbox and needs no
//! tenant process left. An unconfined sandbox verifies no toolbox and cannot
//! count its processes, so a drill on it would skip every checkpoint. This
//! wrapper names one toolbox from every engine, so a replacement process
//! accepts the first one's checkpoint, and reports itself quiet: the drill's
//! turn calls only file tools, which the executor serves itself, so no
//! tenant process ever runs in it.

use std::path::PathBuf;

use af_core::id::Uuid7;
use afr_engine::Result;
use afr_engine::{
    Capabilities, EgressPlan, Engine, Environment, Granted, Killed, Limits, ParkMode, Parked,
    Quiescence, Root, Sandbox, SandboxSpec, SandboxState, Sweep, Swept,
};
use afr_executor::Executor;

use super::UnsandboxedEngine;

/// What every drill toolbox digest is taken over: a label rather than image
/// bytes, so two processes agree on it without sharing a file.
const TOOLBOX_LABEL: &[u8] = b"agentsfleet.recovery-drill-toolbox.v1";

/// Builds unconfined sandboxes that name one toolbox and report no tenant
/// process.
#[derive(Debug)]
pub struct RecoveryDrillEngine {
    unconfined: UnsandboxedEngine,
    toolbox: Box<str>,
}

impl RecoveryDrillEngine {
    /// An engine whose runs live under `base`, as
    /// [`UnsandboxedEngine::new`]'s do.
    ///
    /// # Errors
    /// Always in a release build.
    pub fn new(base: PathBuf) -> crate::Result<Self> {
        Ok(Self {
            unconfined: UnsandboxedEngine::new(base)?,
            toolbox: af_core::etag::sha256_hex(TOOLBOX_LABEL).into(),
        })
    }
}

#[async_trait::async_trait]
impl Engine for RecoveryDrillEngine {
    fn capabilities(&self) -> Capabilities {
        self.unconfined.capabilities()
    }

    fn grant(&self, limits: Limits) -> Result<Granted> {
        self.unconfined.grant(limits)
    }

    fn admission_bound(&self) -> Option<afr_engine::AdmissionBound> {
        self.unconfined.admission_bound()
    }

    async fn prepare(&self, spec: SandboxSpec<'_>) -> Result<Box<dyn Sandbox>> {
        let unconfined = self.unconfined.prepare(spec).await?;
        let environment = Environment {
            root: Root::Toolbox {
                digest: self.toolbox.clone(),
            },
            ..unconfined.environment().clone()
        };
        Ok(Box::new(Drilled {
            unconfined,
            environment,
        }))
    }

    async fn sweep(&self, scope: Sweep<'_>) -> Result<Swept> {
        self.unconfined.sweep(scope).await
    }
}

/// An unconfined sandbox that names the drill's toolbox.
#[derive(Debug)]
struct Drilled {
    unconfined: Box<dyn Sandbox>,
    environment: Environment,
}

#[async_trait::async_trait]
impl Sandbox for Drilled {
    fn id(&self) -> &Uuid7 {
        self.unconfined.id()
    }

    fn environment(&self) -> &Environment {
        &self.environment
    }

    fn granted(&self) -> &Granted {
        self.unconfined.granted()
    }

    fn executor(&self) -> &dyn Executor {
        self.unconfined.executor()
    }

    fn state(&self) -> SandboxState {
        self.unconfined.state()
    }

    /// Quiet: the drill's turn starts no tenant process, and the executor's
    /// own file jobs are proved idle before any capture.
    async fn quiesce(&self) -> Result<Quiescence> {
        Ok(Quiescence::Quiet)
    }

    async fn park(self: Box<Self>, mode: ParkMode) -> Result<Box<dyn Parked>> {
        self.unconfined.park(mode).await
    }

    async fn egress(&mut self, plan: EgressPlan<'_>) -> Result<()> {
        self.unconfined.egress(plan).await
    }

    async fn kill(&self) -> Killed {
        self.unconfined.kill().await
    }

    fn snapshots(&self) -> Option<&dyn afr_engine::Snapshots> {
        None
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        self.unconfined.destroy().await
    }
}

#[cfg(test)]
#[path = "drill_tests.rs"]
mod tests;
