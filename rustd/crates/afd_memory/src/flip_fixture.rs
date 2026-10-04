//! The store the flip suites rig: in memory, refusing one import, or holding
//! one verb until the suite releases it.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::MemoryDelta;
use tokio::sync::Notify;

use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};
use crate::{InMemory, MemoryStore};

/// The verb a [`Rigged`] store holds the first time it is called, until
/// released, having said so on `reached`: an export once it has its snapshot,
/// any other verb before it touches a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hold {
    Nothing,
    Export,
    Upsert,
    Forget,
    Import,
}

/// An in-memory store rigged to fail one import, or to hold one verb.
#[derive(Debug)]
pub(super) struct Rigged {
    pub(super) inner: InMemory,
    /// The import to refuse, counted from one.
    fails_on: Option<usize>,
    imported: AtomicUsize,
    holds: Hold,
    /// Whether the held verb has been held: it is held once.
    spent: AtomicBool,
    pub(super) reached: Notify,
    pub(super) release: Notify,
}

impl Rigged {
    pub(super) fn new(name: &'static str, fails_on: Option<usize>, holds: Hold) -> Self {
        Self {
            inner: InMemory::new(name),
            fails_on,
            imported: AtomicUsize::new(0),
            holds,
            spent: AtomicBool::new(false),
            reached: Notify::new(),
            release: Notify::new(),
        }
    }

    async fn hold(&self, verb: Hold) {
        if verb == self.holds && !self.spent.swap(true, Ordering::SeqCst) {
            self.reached.notify_one();
            self.release.notified().await;
        }
    }
}

#[async_trait::async_trait]
impl MemoryStore for Rigged {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    async fn window(&self, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>> {
        self.inner.window(owner, reads).await
    }

    async fn upsert(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Housekept> {
        self.hold(Hold::Upsert).await;
        self.inner.upsert(owner, entries, now).await
    }

    async fn search(
        &self,
        owner: Owner<'_>,
        reads: bool,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Record>> {
        self.inner.search(owner, reads, query, limit).await
    }

    async fn page(
        &self,
        owner: Owner<'_>,
        reads: bool,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>> {
        self.inner.page(owner, reads, view, after, limit).await
    }

    async fn forget(&self, owner: Owner<'_>, key: &str) -> Result<bool> {
        self.hold(Hold::Forget).await;
        self.inner.forget(owner, key).await
    }

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        let snapshot = self.inner.export(workspace).await;
        self.hold(Hold::Export).await;
        snapshot
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        let nth = self.imported.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fails_on == Some(nth) {
            return Err(crate::Error::refused(self.inner.name()));
        }
        self.hold(Hold::Import).await;
        self.inner.import(workspace, record).await
    }
}
