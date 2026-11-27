//! A sandbox's workspace as a save reads it: the guest agent freezes the
//! tenant leaf, walks the workspace into one spool on the host while every
//! tenant process is held, and thaws it; the spool is read after.
//!
//! The guest agent sits outside the leaf it freezes, so it keeps answering
//! and the VM never stops. A guest agent with no leaf holds nothing, so its
//! sandbox must prove no tenant process runs. The guest agent keeps each
//! pinned entry's identity, so a walk against the generation its last
//! committed pin published copies only what changed; with none for it,
//! everything, once.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use af_chunk_store::Change;
use af_core::id::Uuid7;
use af_guest_wire::protocol::PinWalked;
use af_wire::durable_task::CheckpointEntry;
use futures_util::{StreamExt as _, stream::BoxStream};
use tokio::io::AsyncWriteExt as _;

use super::bounded::Bounded;
use super::pack;
use crate::engine::volume::{Paused, PausedWork, Resumed, WorkspaceVolume};
use crate::engine::{Quiescence, Sandbox};
use crate::error::{Result, unsupported};

/// What an engine with no tenant leaf cannot do while a tenant process runs:
/// the refusal a save reads as busy processes.
pub const FREEZE_TENANT: &str = "freeze a running tenant process";
/// The event a thaw that failed after a failed freeze is logged under.
const EVENT_UNTHAWED: &str = "checkpoint_tenant_unthawed";
/// What every spool's name ends with, so a sweep knows its leftovers.
const SPOOL_SUFFIX: &str = ".pin";

/// The workspace `sandbox` serves, its pins spooled under `spools`, which
/// only the runner reaches, and held to the disk the run was granted.
#[derive(Debug)]
pub struct GuestVolume<'a> {
    sandbox: &'a dyn Sandbox,
    spools: &'a Path,
    /// Whether the pause froze the tenant leaf, so the resume thaws it.
    frozen: AtomicBool,
}

impl<'a> GuestVolume<'a> {
    /// `sandbox`'s workspace, its pins spooled under `spools`.
    #[must_use]
    pub const fn new(sandbox: &'a dyn Sandbox, spools: &'a Path) -> Self {
        Self {
            sandbox,
            spools,
            frozen: AtomicBool::new(false),
        }
    }
}

/// One pin: its spool, what the walk found, and the generation its changes
/// are the difference from, when they are.
#[derive(Debug)]
pub struct GuestPin {
    spool: tempfile::NamedTempFile,
    walked: PinWalked,
    parent: Option<Uuid7>,
}

#[async_trait::async_trait]
impl WorkspaceVolume for GuestVolume<'_> {
    type Pin = GuestPin;

    /// Freezes the tenant leaf. A guest agent with no leaf holds nothing, so
    /// a tenant process its sandbox reports running refuses the pause.
    async fn pause(&self) -> Result<()> {
        let agent = self.sandbox.agent();
        match agent.freeze_tenant().await {
            Ok(()) => {
                self.frozen.store(true, Ordering::SeqCst);
                Ok(())
            }
            Err(failure) if failure.is_no_tenant_leaf() => match self.sandbox.quiesce().await? {
                Quiescence::Quiet => Ok(()),
                Quiescence::Busy => Err(unsupported(
                    self.sandbox.environment().engine,
                    FREEZE_TENANT,
                )),
            },
            // The guest agent thaws a freeze it failed; one whose answer was
            // lost may hold the leaf still.
            Err(failure) => {
                if let Err(stuck) = agent.thaw_tenant().await {
                    let error_code = crate::Error::from(stuck).code();
                    tracing::warn!(event = EVENT_UNTHAWED, %error_code);
                }
                Err(failure.into())
            }
        }
    }

    /// Walks the workspace into a fresh spool, held to the granted disk: a
    /// pin a guest streams past it is refused, and its spool goes.
    async fn pin(&self, _paused: &Paused, parent: Option<&Uuid7>) -> Result<GuestPin> {
        let spool = tempfile::Builder::new()
            .prefix(&format!("{}-", self.sandbox.id()))
            .suffix(SPOOL_SUFFIX)
            .tempfile_in(self.spools)?;
        let mut stream = Bounded::new(spool.reopen()?.into(), self.disk_bytes());
        let label = parent.map(ToString::to_string);
        let agent = self.sandbox.agent();
        let walked = agent.pin_walk(label.as_deref(), &mut stream).await;
        if stream.outgrown() {
            return Err(std::io::Error::from(std::io::ErrorKind::FileTooLarge).into());
        }
        let walked = walked?;
        stream.flush().await?;
        Ok(GuestPin {
            spool,
            parent: parent.filter(|_| walked.delta).cloned(),
            walked,
        })
    }

    /// Thaws the leaf the pause froze, if it froze one.
    async fn resume(&self) -> Result<()> {
        if self.frozen.swap(false, Ordering::SeqCst) {
            self.sandbox.agent().thaw_tenant().await?;
        }
        Ok(())
    }

    /// A removal for each path the parent held that is gone, then every
    /// entry the spool holds, their content held to the granted disk.
    fn changes<'a>(&'a self, pin: &'a Resumed<GuestPin>) -> BoxStream<'a, Result<Change>> {
        let pinned = pin.pin();
        let removed = pinned.walked.removed.iter().map(|path| {
            Ok(Change::Entry(CheckpointEntry::Remove {
                path: path.clone(),
            }))
        });
        let spool = pinned.spool.path().to_owned();
        futures_util::stream::iter(removed)
            .chain(pack::changes(spool, self.disk_bytes()))
            .boxed()
    }

    fn paused_work(&self, pin: &Resumed<GuestPin>) -> PausedWork {
        let walked = &pin.pin().walked;
        PausedWork {
            entries: walked.entries,
            changed_bytes: walked.changed_bytes,
        }
    }

    fn parent<'a>(&self, pin: &'a Resumed<GuestPin>) -> Option<&'a Uuid7> {
        pin.pin().parent.as_ref()
    }

    /// Tells the guest agent which generation the pin committed as, if any,
    /// then removes its spool, whatever the guest agent said.
    async fn release(&self, pin: Resumed<GuestPin>, committed: Option<&Uuid7>) -> Result<()> {
        let GuestPin { spool, walked, .. } = pin.into_pin();
        let label = committed.map(ToString::to_string);
        let agent = self.sandbox.agent();
        let released = agent.pin_release(walked.pin, label.as_deref()).await;
        spool.close()?;
        Ok(released?)
    }
}

impl GuestVolume<'_> {
    /// The disk the run was granted: what a pin's stream, and the content
    /// its files claim, are held to.
    fn disk_bytes(&self) -> u64 {
        self.sandbox.granted().limits().disk_bytes
    }
}
