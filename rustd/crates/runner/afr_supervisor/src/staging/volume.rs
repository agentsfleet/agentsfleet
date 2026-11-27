//! A sandbox's workspace as a save reads it today: pinned by a whole export
//! through its guest agent into one staging file, read back after resume.
//!
//! The pause freezes the guest's tenant leaf through the guest agent, which
//! sits outside it and serves the export while every tenant process, a
//! detached one too, is held; the VM never stops. A guest agent with no leaf
//! holds nothing, so its sandbox must prove no tenant process runs.
//!
//! The export is the only work under the pause. The pin's file is unpacked
//! into a stage beside it on the first read of its changes, which runs after
//! the run resumed, and both go when the pin is released or dropped. A walk
//! that reads only what changed replaces the export without the publication
//! that reads it changing.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use std::sync::atomic::{AtomicBool, Ordering};

use afr_engine::{Paused, Quiescence, Resumed, Sandbox, WorkspaceVolume};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::{Staged, Staging, spool_export};
use crate::recovery::{ChangeSource, Tree};

/// What an engine with no tenant leaf cannot do while a tenant process
/// runs: the refusal a save reads as busy processes.
pub(crate) const FREEZE_TENANT: &str = "freeze a running tenant process";

/// The workspace `sandbox` serves, pinned through files under `staging`.
#[derive(Debug)]
pub(crate) struct ExportVolume<'a> {
    sandbox: &'a dyn Sandbox,
    staging: &'a Staging,
    stop: CancellationToken,
    /// Whether the pause froze the tenant leaf, so the resume thaws it.
    frozen: AtomicBool,
}

impl<'a> ExportVolume<'a> {
    /// `sandbox`'s workspace, staged under `staging`; a read of its changes
    /// fails once `stop` is cancelled.
    pub(crate) const fn new(
        sandbox: &'a dyn Sandbox,
        staging: &'a Staging,
        stop: CancellationToken,
    ) -> Self {
        Self {
            sandbox,
            staging,
            stop,
            frozen: AtomicBool::new(false),
        }
    }
}

/// One save's pinned bytes, and the stage they are read from after resume.
#[derive(Debug)]
pub(crate) struct ExportPin {
    file: tempfile::NamedTempFile,
    stage: OnceCell<(Staged, Tree)>,
    staging: Staging,
    sandbox: Uuid7,
    disk_bytes: u64,
    stop: CancellationToken,
}

impl ExportPin {
    /// The pinned tree, unpacked from the pin's file on its first read.
    async fn tree(&self) -> crate::Result<&Tree> {
        let (_, tree) = self
            .stage
            .get_or_try_init(async || {
                let staged = self.staging.stage(&self.sandbox)?;
                staged.unpack(self.file.reopen()?, self.disk_bytes).await?;
                let tree = Tree::new(staged.path().to_owned(), self.stop.clone());
                Ok::<_, crate::Error>((staged, tree))
            })
            .await?;
        Ok(tree)
    }
}

#[async_trait::async_trait]
impl WorkspaceVolume for ExportVolume<'_> {
    type Pin = ExportPin;

    /// Freezes the tenant leaf; the run's loop, which starts every call,
    /// waits on the save until it resumes. A guest agent with no leaf holds
    /// nothing, so a tenant process its sandbox reports running refuses it.
    async fn pause(&self) -> afr_engine::Result<()> {
        let agent = self.sandbox.agent();
        match agent.freeze_tenant().await {
            Ok(()) => {
                self.frozen.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(failure) if failure.is_no_tenant_leaf() => match self.sandbox.quiesce().await? {
                Quiescence::Quiet => Ok(()),
                Quiescence::Busy => Err(afr_engine::error::unsupported(
                    self.sandbox.environment().engine,
                    FREEZE_TENANT,
                )),
            },
            // The guest agent thaws a freeze it failed; one whose answer was
            // lost may hold the leaf still.
            Err(failure) => {
                if let Err(stuck) = agent.thaw_tenant().await {
                    unthawed(&stuck.into());
                }
                Err(failure.into())
            }
        }
    }

    /// Exports the whole workspace into the pin's file. The walk reads every
    /// byte, so `parent` changes nothing it reads.
    async fn pin(
        &self,
        _paused: &Paused,
        _parent: Option<&Uuid7>,
    ) -> afr_engine::Result<ExportPin> {
        let disk_bytes = self.sandbox.granted().limits().disk_bytes;
        let pinned = async {
            let file = self.staging.pin_file(self.sandbox.id())?;
            spool_export(self.sandbox.agent(), disk_bytes, file.reopen()?).await?;
            Ok::<_, crate::Error>(file)
        };
        let file = pinned.await.map_err(afr_engine::Error::engine)?;
        Ok(ExportPin {
            file,
            stage: OnceCell::new(),
            staging: self.staging.clone(),
            sandbox: self.sandbox.id().clone(),
            disk_bytes,
            stop: self.stop.clone(),
        })
    }

    /// Thaws the leaf the pause froze, if it froze one.
    async fn resume(&self) -> afr_engine::Result<()> {
        if self.frozen.swap(false, Ordering::SeqCst) {
            self.sandbox.agent().thaw_tenant().await?;
        }
        Ok(())
    }

    fn changes<'a>(
        &'a self,
        pin: &'a Resumed<ExportPin>,
    ) -> BoxStream<'a, afr_engine::Result<Change>> {
        futures_util::stream::once(pin.pin().tree())
            .map_ok(ChangeSource::changes)
            .try_flatten()
            .map_err(afr_engine::Error::engine)
            .boxed()
    }

    async fn release(&self, pin: Resumed<ExportPin>, _committed: bool) -> afr_engine::Result<()> {
        let pin = pin.into_pin();
        drop(pin.stage);
        pin.file
            .close()
            .map_err(|failure| afr_engine::Error::engine(crate::Error::from(failure)))
    }
}

/// Logs a thaw that failed after a freeze whose answer was lost.
fn unthawed(failure: &afr_engine::Error) {
    let error_code = failure.code();
    tracing::warn!(event = "checkpoint_tenant_unthawed", %error_code);
}
