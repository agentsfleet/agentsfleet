//! The store the flip suites rig: in memory, over rows another rigged store
//! may share, refusing one kind of call, or holding one verb until the suite
//! releases it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::MemoryDelta;
use tokio::sync::Notify;

use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};
use crate::{Error, InMemory, MemoryStore};

/// The verb a [`Rigged`] store holds the first time it is called, until
/// released, having said so on `reached`: an export once it has its snapshot,
/// any other verb before it touches a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Hold {
    Nothing,
    Export,
    Upsert,
    /// A flip's prune deleting the version it read.
    ForgetStale,
    Import,
}

/// The call a [`Rigged`] store refuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refuse {
    Nothing,
    /// The import counted this far, from one.
    Import(usize),
    /// Every upsert.
    Upsert,
}

/// An in-memory store rigged to refuse a call, or to hold one verb.
#[derive(Debug)]
pub(super) struct Rigged {
    pub(super) inner: Arc<InMemory>,
    refuses: Refuse,
    imported: AtomicUsize,
    holds: Hold,
    /// Whether the held verb has been held: it is held once.
    spent: AtomicBool,
    pub(super) reached: Notify,
    pub(super) release: Notify,
}

impl Rigged {
    pub(super) fn new(name: &'static str, refuses: Refuse, holds: Hold) -> Self {
        Self::sharing(&Arc::new(InMemory::new(name)), refuses, holds)
    }

    /// Another store object over `inner`'s rows: the same rows, a different
    /// identity.
    pub(super) fn sharing(inner: &Arc<InMemory>, refuses: Refuse, holds: Hold) -> Self {
        Self {
            inner: Arc::clone(inner),
            refuses,
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

    fn refusal(&self) -> Error {
        Error::refused(self.inner.name())
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
        if self.refuses == Refuse::Upsert {
            return Err(self.refusal());
        }
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
        self.inner.forget(owner, key).await
    }

    async fn forget_stale(&self, owner: Owner<'_>, key: &str, seen_ms: i64) -> Result<bool> {
        self.hold(Hold::ForgetStale).await;
        self.inner.forget_stale(owner, key, seen_ms).await
    }

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        let snapshot = self.inner.export(workspace).await;
        self.hold(Hold::Export).await;
        snapshot
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        let nth = self.imported.fetch_add(1, Ordering::SeqCst) + 1;
        if self.refuses == Refuse::Import(nth) {
            return Err(self.refusal());
        }
        self.hold(Hold::Import).await;
        self.inner.import(workspace, record).await
    }
}
