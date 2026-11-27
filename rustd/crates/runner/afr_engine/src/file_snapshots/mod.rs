//! Snapshots of a sandbox's workspace as files, published into the chunk
//! store the supervisor handed its build: what an engine whose guest agent
//! pins and renews serves [`crate::Snapshots`] with.
//!
//! ```text
//!   quiesce(parent)  pause ─► pin(parent) ─► resume        tenant code held only here
//!   publish(q, id)   changes ─► the store ─► release(q, id)  after it runs on
//!   take(id)         quiesce(last) ─► publish
//!   restore(ref)   environment? ─► tree ─► pack ─► renew ─► import
//!   forget(ref)    the store stops naming it
//! ```
//!
//! [`quiesce_workspace`] is the one way the supervisor holds a workspace for
//! a save: a sandbox's snapshots where its engine offers them, the same guest
//! agent walk otherwise.
//!
//! A restore refuses a ref from another environment before it reads the
//! store, and reads every chunk before the guest agent renews: a restore that
//! cannot read its tree leaves the workspace as it was. The engine reads its
//! egress plan back after, since only it holds the plan.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use af_chunk_store::ChunkStore;
use af_core::id::Uuid7;
use futures_util::StreamExt as _;

use crate::engine::volume::{Quiesced, quiesce};
use crate::engine::{Sandbox, SnapshotFormat, SnapshotRef};
use crate::error::{Result, SnapshotRefusal, snapshot_refused};

mod bounded;
mod pack;
mod volume;

pub use self::bounded::Bounded;
pub use self::volume::{FREEZE_TENANT, GuestPin, GuestVolume};

/// Where an import lands a restored tree: the whole workspace.
const WHOLE: &str = ".";
/// The event a pin whose release failed after its commit is logged under.
const EVENT_RELEASE_FAILED: &str = "snapshot_pin_release_failed";

/// One sandbox's snapshots: the store its build was handed, if any, where
/// its spools go, and the generation the next take is a delta on.
#[derive(Debug)]
pub struct FileSnapshots {
    store: Option<Arc<dyn ChunkStore>>,
    spools: PathBuf,
    /// The last generation a take committed, while the workspace still holds
    /// what it saved and the store still names it.
    last: Mutex<Option<Uuid7>>,
}

impl FileSnapshots {
    /// Snapshots published into `store`, each pin spooled under `spools`,
    /// which only the runner reaches. With no store, a sandbox still
    /// quiesces for the daemon's publication, and takes and restores nothing.
    #[must_use]
    pub const fn new(store: Option<Arc<dyn ChunkStore>>, spools: PathBuf) -> Self {
        Self {
            store,
            spools,
            last: Mutex::new(None),
        }
    }

    /// Holds `sandbox`'s tenant code only for the pin against `parent`, and
    /// returns once it runs on.
    ///
    /// # Errors
    /// As [`quiesce`].
    pub async fn quiesce<'a>(
        &'a self,
        sandbox: &'a dyn Sandbox,
        parent: Option<&Uuid7>,
    ) -> Result<Box<dyn Quiesced + 'a>> {
        let volume = GuestVolume::new(sandbox, &self.spools);
        Ok(Box::new(quiesce(volume, parent).await?))
    }

    /// Publishes what `quiesced` fixed of `sandbox` under `id` into the
    /// store, then releases it naming `id` when the commit held.
    ///
    /// # Errors
    /// No store, or the publication failed; nothing is named `id`.
    pub async fn publish(
        &self,
        sandbox: &dyn Sandbox,
        quiesced: Box<dyn Quiesced + '_>,
        id: Uuid7,
    ) -> Result<SnapshotRef> {
        let published = self.published(&*quiesced, &id).await;
        let committed = published.as_ref().ok().map(|_| &id);
        // A release that fails costs the next take its delta, never a save.
        if let Err(failure) = quiesced.release(committed).await {
            let error_code = failure.code();
            tracing::warn!(event = EVENT_RELEASE_FAILED, %error_code);
        }
        let generation = published?;
        *self.last() = Some(id.clone());
        let environment = sandbox.environment().clone();
        Ok(SnapshotRef::new(
            id,
            environment,
            generation,
            SnapshotFormat::Files,
        ))
    }

    /// Saves `sandbox`'s workspace under `id`, a delta on the last take.
    ///
    /// # Errors
    /// As [`FileSnapshots::quiesce`] and [`FileSnapshots::publish`].
    pub async fn take(&self, sandbox: &dyn Sandbox, id: Uuid7) -> Result<SnapshotRef> {
        let parent = self.last().clone();
        let quiesced = self.quiesce(sandbox, parent.as_ref()).await?;
        self.publish(sandbox, quiesced, id).await
    }

    /// Replaces `sandbox`'s workspace with what `snapshot` saved, ending
    /// every process of the environment before it.
    ///
    /// # Errors
    /// `snapshot` was taken on another environment, refused before the store
    /// is read; no store; the store names another generation under its
    /// identifier, or none; or a read, the renewal or the import failed.
    pub async fn restore(&self, sandbox: &dyn Sandbox, snapshot: &SnapshotRef) -> Result<()> {
        snapshot.restores_on(sandbox.environment())?;
        let store = self.store()?;
        let stored = store.tree(snapshot.id()).await?;
        if stored.generation != snapshot.generation() {
            return Err(snapshot_refused(SnapshotRefusal::Generation));
        }
        let packed = pack::pack(&stored.tree, &**store, &self.spools).await?;
        let agent = sandbox.agent();
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
    /// No store, or it names nothing under the snapshot's identifier.
    pub async fn forget(&self, snapshot: &SnapshotRef) -> Result<()> {
        self.store()?.forget(snapshot.id()).await?;
        let mut last = self.last();
        if last.as_ref() == Some(snapshot.id()) {
            *last = None;
        }
        Ok(())
    }

    /// Publishes `quiesced`'s changes under `id`; the generation it committed as.
    async fn published(&self, quiesced: &dyn Quiesced, id: &Uuid7) -> Result<u64> {
        let mut publication = self.store()?.publish(id, quiesced.parent()).await?;
        let mut changes = quiesced.changes();
        while let Some(change) = changes.next().await {
            publication.add(change?).await?;
        }
        Ok(publication.commit().await?)
    }

    /// The store, or the refusal of a sandbox whose build was handed none.
    fn store(&self) -> Result<&Arc<dyn ChunkStore>> {
        self.store
            .as_ref()
            .ok_or(SnapshotRefusal::Store)
            .map_err(snapshot_refused)
    }

    fn last(&self) -> MutexGuard<'_, Option<Uuid7>> {
        // Nothing panics while the lock is held, so a poisoned one still
        // holds a whole value.
        self.last.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Holds `sandbox` still for a pin against `parent` and returns once it runs
/// on: through its snapshots when its engine offers them, otherwise through
/// its guest agent's walk, spooled under `spools`.
///
/// # Errors
/// As [`quiesce`].
pub async fn quiesce_workspace<'a>(
    sandbox: &'a dyn Sandbox,
    spools: &'a Path,
    parent: Option<&Uuid7>,
) -> Result<Box<dyn Quiesced + 'a>> {
    match sandbox.snapshots() {
        Some(snapshots) => snapshots.quiesce(parent).await,
        None => Ok(Box::new(
            quiesce(GuestVolume::new(sandbox, spools), parent).await?,
        )),
    }
}

/// `bytes` as a count of bytes: a length this host holds always fits.
fn byte_count(bytes: usize) -> u64 {
    u64::try_from(bytes).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests;
