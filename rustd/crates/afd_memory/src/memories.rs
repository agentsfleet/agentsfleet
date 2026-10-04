//! The handle `agentsfleetd` holds once: every memory verb, the grants each
//! enforces, and the route to each workspace's store.
//!
//! ```text
//!   lease plane ─┐                          ┌─► Postgres (today)
//!   runner route ├─► Memories ─► grants ─► route ┤
//!   tenant route ┘   (core.fleets)          └─► the store a flip moved it to
//! ```

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_observability::producers::memory;
use afd_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use afd_wire::memory::MemoryDelta;

use crate::access::{Access, Directory};
use crate::admit::admit;
use crate::error::{Result, entry_not_found, fleet_not_found, moving};
use crate::page::{After, View};
use crate::postgres::PgStore;
use crate::record::{Housekept, Owner, Record};
use crate::route::Routes;
use crate::store::MemoryStore;

/// An admin changed a fleet's shared-memory grants.
const EVENT_ACCESS_CHANGED: &str = "memory_access_changed";

/// Every memory verb `agentsfleetd` serves, over one route table.
///
/// Cheap to clone: one `Arc` (`M-SERVICES-CLONE`), so the lease plane and both
/// route families share one route table and one view of every flip.
#[derive(Debug, Clone)]
pub struct Memories {
    inner: Arc<Inner>,
}

#[derive(Debug)]
struct Inner {
    directory: Directory,
    routes: Routes,
}

/// What one capture wrote, and what it declined to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Captured {
    /// Entries upserted.
    pub stored: usize,
    /// Entries refused for shape, and shares from a fleet that may not publish.
    pub skipped: usize,
    /// Entries beyond the push byte cap, which end the batch.
    pub truncated: usize,
    /// Of `skipped`, the shares refused for want of the publish grant.
    pub unpublished: usize,
    /// Rows the retention sweep removed.
    pub swept: u64,
    /// Rows evicted to bring the fleet back under its cap.
    pub evicted: u64,
}

impl Captured {
    /// Records what this capture stored and what it would not.
    fn record(&self) {
        let count = |size: usize| u64::try_from(size).unwrap_or(u64::MAX);
        memory::captured(count(self.stored));
        memory::capture_skipped(count(self.skipped));
        memory::capture_truncated(count(self.truncated));
        memory::cap_evicted(self.evicted);
    }
}

impl Memories {
    /// Every workspace's memory in Postgres, through `database`.
    #[must_use]
    pub fn new(database: Db, entropy: Entropy) -> Self {
        let store = Arc::new(PgStore::new(database.clone(), entropy));
        Self::over(database, store)
    }

    /// Every workspace's memory in `store`, with the grants read through
    /// `database`.
    #[must_use]
    pub fn over(database: Db, store: Arc<dyn MemoryStore>) -> Self {
        Self {
            inner: Arc::new(Inner {
                directory: Directory::new(database),
                routes: Routes::new(store),
            }),
        }
    }

    pub(crate) fn routes(&self) -> &Routes {
        &self.inner.routes
    }

    pub(crate) fn directory(&self) -> &Directory {
        &self.inner.directory
    }

    /// `fleet`'s workspace and grants, or the refusal for a fleet that is gone.
    pub(crate) async fn access(&self, fleet: &Uuid7) -> Result<Access> {
        self.directory()
            .of(fleet)
            .await?
            .ok_or_else(fleet_not_found)
    }

    /// As [`Self::access`], refusing a fleet `workspace` does not hold with the
    /// same answer as one that does not exist.
    async fn access_in(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<Access> {
        Some(self.access(fleet).await?)
            .filter(|access| &access.workspace == workspace)
            .ok_or_else(fleet_not_found)
    }

    /// Persist what one run learned, and sweep and cap the fleet.
    ///
    /// # Errors
    /// Refuses a fleet that is gone and reports a store that would not take
    /// the write. A delta refused for its shape, or a share from a fleet that
    /// may not publish, is counted rather than refused.
    pub async fn capture(
        &self,
        fleet: &Uuid7,
        deltas: &[MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Captured> {
        let captured = self.capture_counted(fleet, deltas, now).await;
        match captured {
            Ok(ref counted) => counted.record(),
            Err(ref _refused) => memory::push_failed(),
        }
        captured
    }

    async fn capture_counted(
        &self,
        fleet: &Uuid7,
        deltas: &[MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Captured> {
        let Access { workspace, grants } = self.access(fleet).await?;
        let admitted = admit(deltas, grants.publish);
        let owner = Owner {
            workspace: &workspace,
            fleet,
        };
        let entries = admitted.entries.as_slice();
        let housekept = self.upsert_through(owner, entries, now).await?;
        Ok(Captured {
            stored: entries.len(),
            skipped: admitted.skipped + admitted.unpublished,
            truncated: admitted.truncated,
            unpublished: admitted.unpublished,
            swept: housekept.swept,
            evicted: housekept.evicted,
        })
    }

    /// One operator page of `fleet`'s memory, holding the workspace's shared
    /// entries too when the fleet may read them.
    ///
    /// # Errors
    /// Refuses a fleet `workspace` does not hold, and reports a store that
    /// would not answer.
    pub async fn page(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>> {
        let access = self.access_in(workspace, fleet).await?;
        let owner = Owner { workspace, fleet };
        self.routes()
            .of(workspace)
            .store
            .page(owner, access.grants.read, view, after, limit)
            .await
    }

    /// Removes one of `fleet`'s own entries; another fleet's entry under the
    /// same key is never touched.
    ///
    /// # Errors
    /// Refuses a fleet `workspace` does not hold, a key the fleet is not
    /// holding, and a forget that would race a flip's copy — a copy could put
    /// the entry back, so the caller is told to try again.
    pub async fn forget(&self, workspace: &Uuid7, fleet: &Uuid7, key: &str) -> Result<()> {
        self.access_in(workspace, fleet).await?;
        let route = self.routes().of(workspace);
        if let Some(mirror) = &route.mirror {
            return Err(moving(mirror.name()));
        }
        let owner = Owner { workspace, fleet };
        let forgotten = route.store.forget(owner, key).await?;
        if !Arc::ptr_eq(&route, &self.routes().of(workspace)) {
            return Err(moving(route.store.name()));
        }
        forgotten.then_some(()).ok_or_else(entry_not_found)
    }

    /// Sets `fleet`'s shared-memory grants, answering both as they now stand.
    ///
    /// # Errors
    /// Refuses a fleet `workspace` does not hold, and reports a database that
    /// would not answer.
    pub async fn set_access(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        change: MemoryAccessRequest,
    ) -> Result<MemoryAccess> {
        let access = self
            .directory()
            .set(workspace, fleet, change)
            .await?
            .ok_or_else(fleet_not_found)?;
        if change.read.is_none() && change.publish.is_none() {
            return Ok(access);
        }
        let (workspace_id, fleet_id) = (workspace.as_str(), fleet.as_str());
        let MemoryAccess { read, publish } = access;
        tracing::info!(
            workspace_id,
            fleet_id,
            read,
            publish,
            event = EVENT_ACCESS_CHANGED,
            "a fleet's shared-memory grants changed"
        );
        Ok(access)
    }

    /// Upserts `entries` into every store `owner`'s workspace route writes to
    /// (the route's own store, then a flip's mirror) and again into any store
    /// a flip adds before the route holds still, answering what the route's
    /// own store housekept.
    ///
    /// The store being left is written first, so a row a write puts in the
    /// store being filled is already in the one being left. A write that then
    /// fails on a store it still had to reach marks the route it went
    /// through, so the flip filling that store does not switch to it, and the
    /// caller is told the write failed.
    ///
    /// No lock: a write that saw the route before a flip began finds the new
    /// route when it looks again, and writes the store it missed. Each write
    /// is an upsert at one instant, so writing a store twice changes nothing.
    pub(crate) async fn upsert_through(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Housekept> {
        let mut route = self.routes().of(owner.workspace);
        let mut written: Vec<Arc<dyn MemoryStore>> = Vec::with_capacity(2);
        let answer = route.store.upsert(owner, entries, now).await?;
        written.push(Arc::clone(&route.store));
        loop {
            for store in route.writers() {
                if written.iter().any(|done| Arc::ptr_eq(done, store)) {
                    continue;
                }
                store
                    .upsert(owner, entries, now)
                    .await
                    .inspect_err(|_| route.miss())?;
                written.push(Arc::clone(store));
            }
            let current = self.routes().of(owner.workspace);
            if Arc::ptr_eq(&current, &route) {
                return Ok(answer);
            }
            route = current;
        }
    }
}
