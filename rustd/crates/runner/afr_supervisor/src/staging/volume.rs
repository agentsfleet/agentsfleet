//! A sandbox's workspace as a save reads it today: pinned by a whole export
//! through its executor into one staging file, read back after resume.
//!
//! The export is the only work under the pause. The pin's file is unpacked
//! into a stage beside it on the first read of its changes, which runs after
//! the run resumed, and both go when the pin is released or dropped. A walk
//! that reads only what changed replaces the export without the publication
//! that reads it changing.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use afr_engine::{Paused, Resumed, Sandbox, WorkspaceVolume};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::{Staged, Staging, spool_export};
use crate::recovery::{ChangeSource, Tree};

/// The workspace `sandbox` serves, pinned through files under `staging`.
#[derive(Debug)]
pub(crate) struct ExportVolume<'a> {
    sandbox: &'a dyn Sandbox,
    staging: &'a Staging,
    stop: CancellationToken,
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

    /// Holds nothing: the save proved no tenant process remains before it
    /// paused, and the run's loop waits on the save until it resumes.
    async fn pause(&self) -> afr_engine::Result<()> {
        Ok(())
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
            spool_export(self.sandbox.executor(), disk_bytes, file.reopen()?).await?;
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

    async fn resume(&self) -> afr_engine::Result<()> {
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
