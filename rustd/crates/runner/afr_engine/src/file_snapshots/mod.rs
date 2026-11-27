//! Snapshots of a sandbox's workspace as files, published into the chunk
//! store the supervisor handed its build: what an engine whose guest agent
//! pins and renews serves [`crate::Snapshots`] with.
//!
//! ```text
//!   take(id)       pause ─► pin(last) ─► resume ─► publish ─► release(pin, id)
//!   restore(ref)   environment? ─► tree ─► pack ─► renew ─► import
//!   forget(ref)    the store stops naming it
//! ```
//!
//! A restore refuses a ref from another environment before it reads the
//! store, and reads every chunk before the guest agent renews: a restore that
//! cannot read its tree leaves the workspace as it was. The engine reads its
//! egress plan back after, since only it holds the plan.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use af_chunk_store::ChunkStore;
use af_core::id::Uuid7;
use afr_guest_client::GuestAgent;
use futures_util::StreamExt as _;

use self::volume::GuestVolume;
use crate::engine::volume::{Resumed, WorkspaceVolume as _, pin_paused};
use crate::engine::{Environment, SnapshotFormat, SnapshotRef};
use crate::error::{Result, SnapshotRefusal, snapshot_refused};

mod bounded;
mod pack;
mod volume;

pub use self::bounded::Bounded;

/// Where an import lands a restored tree: the whole workspace.
const WHOLE: &str = ".";
/// The event a pin whose release failed after its commit is logged under.
const EVENT_RELEASE_FAILED: &str = "snapshot_pin_release_failed";

/// One sandbox's snapshots: its store, where its spools go, and the
/// generation the next take is a delta on.
#[derive(Debug)]
pub struct FileSnapshots {
    store: Arc<dyn ChunkStore>,
    spools: PathBuf,
    disk_bytes: u64,
    /// The last generation a take committed, while the workspace still holds
    /// what it saved and the store still names it.
    last: Mutex<Option<Uuid7>>,
}

impl FileSnapshots {
    /// Snapshots published into `store`, each pin spooled under `spools`,
    /// which only the runner reaches, and held to `disk_bytes`.
    #[must_use]
    pub const fn new(store: Arc<dyn ChunkStore>, spools: PathBuf, disk_bytes: u64) -> Self {
        Self {
            store,
            spools,
            disk_bytes,
            last: Mutex::new(None),
        }
    }

    /// Saves the workspace `agent` serves under `id`, as taken on
    /// `environment`: tenant code is held only for the pin.
    ///
    /// # Errors
    /// The pause, the pin or the publication failed; nothing is named `id`.
    pub async fn take(
        &self,
        agent: &dyn GuestAgent,
        environment: &Environment,
        id: Uuid7,
    ) -> Result<SnapshotRef> {
        let volume = self.volume(agent);
        let parent = self.last().clone();
        let pin = pin_paused(&volume, parent.as_ref()).await?;
        let published = self.publish(&volume, &pin, &id).await;
        let committed = published.as_ref().ok().map(|_| &id);
        // A release that fails costs the next take its delta, never a save.
        if let Err(failure) = volume.release(pin, committed).await {
            let error_code = failure.code();
            tracing::warn!(event = EVENT_RELEASE_FAILED, %error_code);
        }
        let generation = published?;
        *self.last() = Some(id.clone());
        Ok(SnapshotRef::new(
            id,
            environment.clone(),
            generation,
            SnapshotFormat::Files,
        ))
    }

    /// Replaces the workspace `agent` serves on `environment` with what
    /// `snapshot` saved, ending every process of the environment before it.
    ///
    /// # Errors
    /// `snapshot` was taken on another environment, refused before the store
    /// is read; the store names another generation under its identifier, or
    /// none; or a read, the renewal or the import failed.
    pub async fn restore(
        &self,
        agent: &dyn GuestAgent,
        environment: &Environment,
        snapshot: &SnapshotRef,
    ) -> Result<()> {
        snapshot.restores_on(environment)?;
        let stored = self.store.tree(snapshot.id()).await?;
        if stored.generation != snapshot.generation() {
            return Err(snapshot_refused(SnapshotRefusal::Generation));
        }
        let packed = pack::pack(&stored.tree, &*self.store, &self.spools).await?;
        agent.renew().await?;
        // The renewed workspace holds nothing the last take's pin saw.
        *self.last() = None;
        agent
            .import(WHOLE, &mut tokio::fs::File::from_std(packed))
            .await?;
        Ok(())
    }

    /// Drops `snapshot` from the store; the next take is whole when it was
    /// the last one taken.
    ///
    /// # Errors
    /// The store names nothing under its identifier.
    pub async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.store.forget(snapshot.id()).await?;
        let mut last = self.last();
        if last.as_ref() == Some(snapshot.id()) {
            *last = None;
        }
        Ok(())
    }

    /// Publishes `pin`'s changes under `id`; the generation it committed as.
    async fn publish(
        &self,
        volume: &GuestVolume<'_>,
        pin: &Resumed<volume::GuestPin>,
        id: &Uuid7,
    ) -> Result<u64> {
        let mut publication = self.store.publish(id, volume.parent(pin)).await?;
        let mut changes = volume.changes(pin);
        while let Some(change) = changes.next().await {
            publication.add(change?).await?;
        }
        Ok(publication.commit().await?)
    }

    fn volume<'a>(&'a self, agent: &'a dyn GuestAgent) -> GuestVolume<'a> {
        GuestVolume {
            agent,
            spools: &self.spools,
            disk_bytes: self.disk_bytes,
        }
    }

    fn last(&self) -> MutexGuard<'_, Option<Uuid7>> {
        // Nothing panics while the lock is held, so a poisoned one still
        // holds a whole value.
        self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// `bytes` as a count of bytes: a length this host holds always fits.
fn byte_count(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
