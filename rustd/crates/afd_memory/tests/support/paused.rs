//! A store that holds one verb until the suite releases it, so a flip can land
//! at the exact point a capture, a forget, a copy or a prune is waiting. It
//! wraps the in-memory store, or any other: the live suite wraps Postgres.

use std::sync::atomic::{AtomicBool, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_memory::page::{After, View};
use afd_memory::{Housekept, InMemory, MemoryStore, Owner, Record, Result};
use afd_wire::memory::MemoryDelta;
use tokio::sync::Notify;

/// The verb a [`Paused`] store holds, before it touches a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verb {
    Export,
    Upsert,
    Forget,
    /// A flip's prune deleting the version it read.
    ForgetStale,
}

/// A store whose `holds` verb waits on `release` the first time it is called,
/// having said so on `reached`.
#[derive(Debug)]
pub(crate) struct Paused<S = InMemory> {
    pub(crate) inner: S,
    holds: Verb,
    /// Whether the held verb has been held: a flip exports twice, to prune
    /// and to copy, and only the first waits.
    spent: AtomicBool,
    pub(crate) reached: Notify,
    pub(crate) release: Notify,
}

impl Paused {
    pub(crate) fn new(name: &'static str, holds: Verb) -> Self {
        Self::over(InMemory::new(name), holds)
    }
}

impl<S> Paused<S> {
    pub(crate) fn over(inner: S, holds: Verb) -> Self {
        Self {
            inner,
            holds,
            spent: AtomicBool::new(false),
            reached: Notify::new(),
            release: Notify::new(),
        }
    }

    async fn hold(&self, verb: Verb) {
        if verb == self.holds && !self.spent.swap(true, Ordering::SeqCst) {
            self.reached.notify_one();
            self.release.notified().await;
        }
    }
}

#[async_trait::async_trait]
impl<S: MemoryStore> MemoryStore for Paused<S> {
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
        self.hold(Verb::Upsert).await;
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
        self.hold(Verb::Forget).await;
        self.inner.forget(owner, key).await
    }

    async fn forget_stale(&self, owner: Owner<'_>, key: &str, seen_ms: i64) -> Result<bool> {
        self.hold(Verb::ForgetStale).await;
        self.inner.forget_stale(owner, key, seen_ms).await
    }

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        self.hold(Verb::Export).await;
        self.inner.export(workspace).await
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        self.inner.import(workspace, record).await
    }
}
