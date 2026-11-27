//! A sandbox's workspace as a save reads it: pinned by a walk in its guest
//! agent that copies only what changed into one staging file, read back
//! after resume.
//!
//! The pause freezes the guest's tenant leaf through the guest agent, which
//! sits outside it and walks the workspace while every tenant process, a
//! detached one too, is held; the VM never stops. A guest agent with no leaf
//! holds nothing, so its sandbox must prove no tenant process runs.
//!
//! The walk is the only work under the pause: one identity read per entry,
//! and a copy of each entry whose identity changed since the pin that
//! published `parent`. The guest agent keeps those identities, so a walk with
//! none for `parent` (a new connection, a new host) copies everything once.
//! The pin's file is unpacked into a stage beside it on the first read of its
//! changes, which runs after the run resumed, and both go when the pin is
//! released or dropped.

use af_chunk_store::Change;
use af_core::id::Uuid7;
use af_guest_wire::protocol::PinWalked;
use af_wire::durable_task::CheckpointEntry;
use std::sync::atomic::{AtomicBool, Ordering};

use afr_engine::{Paused, PausedWork, Quiescence, Resumed, Sandbox, WorkspaceVolume};
use futures_util::{StreamExt as _, TryStreamExt as _, stream::BoxStream};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use super::{Staged, Staging, spool_pin};
use crate::recovery::{ChangeSource, Tree};

/// What an engine with no tenant leaf cannot do while a tenant process
/// runs: the refusal a save reads as busy processes.
pub(crate) const FREEZE_TENANT: &str = "freeze a running tenant process";

/// The workspace `sandbox` serves, pinned through files under `staging`.
#[derive(Debug)]
pub(crate) struct WalkVolume<'a> {
    sandbox: &'a dyn Sandbox,
    staging: &'a Staging,
    stop: CancellationToken,
    /// Whether the pause froze the tenant leaf, so the resume thaws it.
    frozen: AtomicBool,
}

impl<'a> WalkVolume<'a> {
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

/// One save's pinned bytes, what the walk found, and the stage they are read
/// from after resume.
#[derive(Debug)]
pub(crate) struct WalkPin {
    file: tempfile::NamedTempFile,
    stage: OnceCell<(Staged, Tree)>,
    staging: Staging,
    sandbox: Uuid7,
    disk_bytes: u64,
    stop: CancellationToken,
    /// The generation the changes are the difference from, when they are.
    parent: Option<Uuid7>,
    walked: PinWalked,
}

impl WalkPin {
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

    /// A removal for each path the parent held that is gone.
    fn removed(&self) -> impl Iterator<Item = Change> + '_ {
        self.walked
            .removed
            .iter()
            .map(|path| Change::Entry(CheckpointEntry::Remove { path: path.clone() }))
    }
}

#[async_trait::async_trait]
impl WorkspaceVolume for WalkVolume<'_> {
    type Pin = WalkPin;

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

    /// Walks the workspace in the guest agent and copies into the pin's file
    /// what changed since `parent`, or everything when the guest agent holds
    /// no identities for it.
    async fn pin(&self, _paused: &Paused, parent: Option<&Uuid7>) -> afr_engine::Result<WalkPin> {
        let disk_bytes = self.sandbox.granted().limits().disk_bytes;
        let label = parent.map(ToString::to_string);
        let pinned = async {
            let file = self.staging.pin_file(self.sandbox.id())?;
            let walked = spool_pin(
                self.sandbox.agent(),
                label.as_deref(),
                disk_bytes,
                file.reopen()?,
            )
            .await?;
            Ok::<_, crate::Error>((file, walked))
        };
        let (file, walked) = pinned.await.map_err(afr_engine::Error::engine)?;
        Ok(WalkPin {
            file,
            stage: OnceCell::new(),
            staging: self.staging.clone(),
            sandbox: self.sandbox.id().clone(),
            disk_bytes,
            stop: self.stop.clone(),
            parent: parent.filter(|_| walked.delta).cloned(),
            walked,
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
        pin: &'a Resumed<WalkPin>,
    ) -> BoxStream<'a, afr_engine::Result<Change>> {
        let pinned = pin.pin();
        let removed = futures_util::stream::iter(pinned.removed().map(Ok));
        let tree = futures_util::stream::once(pinned.tree())
            .map_ok(ChangeSource::changes)
            .try_flatten();
        removed
            .chain(tree)
            .map_err(afr_engine::Error::engine)
            .boxed()
    }

    fn paused_work(&self, pin: &Resumed<WalkPin>) -> PausedWork {
        let walked = &pin.pin().walked;
        PausedWork {
            entries: walked.entries,
            changed_bytes: walked.changed_bytes,
        }
    }

    fn parent<'a>(&self, pin: &'a Resumed<WalkPin>) -> Option<&'a Uuid7> {
        pin.pin().parent.as_ref()
    }

    /// Tells the guest agent whether the pin's generation was committed,
    /// then removes the pin's file and stage, whatever the guest agent said.
    async fn release(
        &self,
        pin: Resumed<WalkPin>,
        committed: Option<&Uuid7>,
    ) -> afr_engine::Result<()> {
        let pin = pin.into_pin();
        let generation = committed.map(ToString::to_string);
        let released = self
            .sandbox
            .agent()
            .pin_release(pin.walked.pin, generation.as_deref())
            .await;
        drop(pin.stage);
        pin.file
            .close()
            .map_err(|failure| afr_engine::Error::engine(crate::Error::from(failure)))?;
        Ok(released?)
    }
}

/// Logs a thaw that failed after a freeze whose answer was lost.
fn unthawed(failure: &afr_engine::Error) {
    let error_code = failure.code();
    tracing::warn!(event = "checkpoint_tenant_unthawed", %error_code);
}
