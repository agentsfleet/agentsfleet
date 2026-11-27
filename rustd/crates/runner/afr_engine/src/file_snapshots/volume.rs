//! A sandbox's workspace as a snapshot reads it: the guest agent freezes the
//! tenant leaf, walks the workspace into one spool on the host while every
//! tenant process is held, and thaws it; the spool is read after.
//!
//! The guest agent sits outside the leaf it freezes, so it keeps answering
//! and the VM never stops. It keeps each pinned entry's identity, so a walk
//! against the generation its last committed pin published copies only what
//! changed; with none for it, everything, once.

use std::path::Path;

use af_chunk_store::Change;
use af_core::id::Uuid7;
use af_guest_wire::protocol::PinWalked;
use af_wire::durable_task::CheckpointEntry;
use afr_guest_client::GuestAgent;
use futures_util::{StreamExt as _, stream::BoxStream};
use tokio::io::AsyncWriteExt as _;

use super::bounded::Bounded;
use super::pack;
use crate::engine::volume::{Paused, PausedWork, Resumed, WorkspaceVolume};
use crate::error::Result;

/// What every spool's name starts with, so a sweep knows its leftovers.
const SPOOL_PREFIX: &str = "snapshot-";

/// The workspace `agent` serves, its pins spooled under `spools` and held to
/// `disk_bytes`, the disk the run was granted.
#[derive(Debug)]
pub(super) struct GuestVolume<'a> {
    pub(super) agent: &'a dyn GuestAgent,
    pub(super) spools: &'a Path,
    pub(super) disk_bytes: u64,
}

/// One pin: its spool, what the walk found, and the generation its changes
/// are the difference from, when they are.
#[derive(Debug)]
pub(super) struct GuestPin {
    spool: tempfile::NamedTempFile,
    walked: PinWalked,
    parent: Option<Uuid7>,
}

#[async_trait::async_trait]
impl WorkspaceVolume for GuestVolume<'_> {
    type Pin = GuestPin;

    async fn pause(&self) -> Result<()> {
        Ok(self.agent.freeze_tenant().await?)
    }

    /// Walks the workspace into a fresh spool, held to the granted disk: a
    /// pin a guest streams past it is refused, and its spool goes.
    async fn pin(&self, _paused: &Paused, parent: Option<&Uuid7>) -> Result<GuestPin> {
        let spool = tempfile::Builder::new()
            .prefix(SPOOL_PREFIX)
            .tempfile_in(self.spools)?;
        let mut stream = Bounded::new(spool.reopen()?.into(), self.disk_bytes);
        let label = parent.map(ToString::to_string);
        let walked = self.agent.pin_walk(label.as_deref(), &mut stream).await;
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

    async fn resume(&self) -> Result<()> {
        Ok(self.agent.thaw_tenant().await?)
    }

    /// A removal for each path the parent held that is gone, then every
    /// entry the spool holds.
    fn changes<'a>(&'a self, pin: &'a Resumed<GuestPin>) -> BoxStream<'a, Result<Change>> {
        let pinned = pin.pin();
        let removed = pinned.walked.removed.iter().map(|path| {
            Ok(Change::Entry(CheckpointEntry::Remove {
                path: path.clone(),
            }))
        });
        futures_util::stream::iter(removed)
            .chain(pack::changes(pinned.spool.path().to_owned()))
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
        let released = self.agent.pin_release(walked.pin, label.as_deref()).await;
        spool.close()?;
        Ok(released?)
    }
}
