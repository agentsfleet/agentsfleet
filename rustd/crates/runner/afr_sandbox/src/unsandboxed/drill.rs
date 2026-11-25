//! An unconfined engine whose workspaces a recovery drill can checkpoint
//! (M-TEST-UTIL).
//!
//! Every run holds a sandbox, and a workspace checkpoint names the sandbox's
//! verified toolbox, freezes it and proves no tenant process remains. An
//! unconfined sandbox verifies no toolbox and refuses the freeze, so a drill
//! on it would skip every checkpoint. This wrapper names one toolbox from
//! every engine, so a replacement process accepts the first one's checkpoint,
//! and answers the freeze and the quiescence proof at once: the drill's turn
//! calls only file tools, which the executor serves itself, so no tenant
//! process ever runs in it to stop.

use std::path::PathBuf;

use afr_executor::Executor;

use super::UnsandboxedEngine;
use crate::engine::{Engine, HostWorkspace, Killed, Sandbox, SandboxRequest};
use crate::error::Result;
use crate::network::Allowlist;

/// What every drill toolbox digest is taken over: a label rather than image
/// bytes, so two processes agree on it without sharing a file.
const TOOLBOX_LABEL: &[u8] = b"agentsfleet.recovery-drill-toolbox.v1";

/// Builds unconfined sandboxes that report one toolbox and freeze at once.
#[derive(Debug)]
pub struct RecoveryDrillEngine {
    unconfined: UnsandboxedEngine,
    toolbox: String,
}

impl RecoveryDrillEngine {
    /// An engine whose runs live under `base`, as
    /// [`UnsandboxedEngine::new`]'s do.
    ///
    /// # Errors
    /// Always in a release build.
    pub fn new(base: PathBuf) -> Result<Self> {
        Ok(Self {
            unconfined: UnsandboxedEngine::new(base)?,
            toolbox: af_core::etag::sha256_hex(TOOLBOX_LABEL),
        })
    }
}

#[async_trait::async_trait]
impl Engine for RecoveryDrillEngine {
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>> {
        let unconfined = self.unconfined.prepare(request).await?;
        Ok(Box::new(Drilled {
            unconfined,
            toolbox: self.toolbox.clone(),
        }))
    }
}

/// An unconfined sandbox that names the drill's toolbox.
#[derive(Debug)]
struct Drilled {
    unconfined: Box<dyn Sandbox>,
    toolbox: String,
}

#[async_trait::async_trait]
impl Sandbox for Drilled {
    fn toolbox_digest(&self) -> Option<&str> {
        Some(&self.toolbox)
    }

    fn executor(&self) -> &dyn Executor {
        self.unconfined.executor()
    }

    fn workspace(&self) -> Option<HostWorkspace<'_>> {
        self.unconfined.workspace()
    }

    fn is_running(&mut self) -> bool {
        self.unconfined.is_running()
    }

    /// Nothing to stop: the drill's turn starts no tenant process, and the
    /// executor's own file jobs are proved idle before any freeze.
    async fn freeze(&self) -> Result<()> {
        Ok(())
    }

    /// Holds for the same reason the freeze answers at once.
    async fn recovery_quiescent(&self) -> Result<bool> {
        Ok(true)
    }

    async fn thaw(&self) -> Result<()> {
        Ok(())
    }

    async fn reallow(&mut self, allowlist: &Allowlist) -> Result<()> {
        self.unconfined.reallow(allowlist).await
    }

    async fn kill(&self) -> Killed {
        self.unconfined.kill().await
    }

    async fn destroy(self: Box<Self>) -> Result<()> {
        self.unconfined.destroy().await
    }
}

#[cfg(test)]
#[path = "drill_tests.rs"]
mod tests;
