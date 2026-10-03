//! Dimensions 2.1–2.3: a flip copies every entry, a failed copy keeps the old
//! store, and a push racing the copy reaches both stores.
//!
//! Against the in-memory store, so the interleaving is the test's to force: a
//! store that pauses its export holds the copy at the exact point a push has
//! to survive.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::test_util::unreachable_db;
use afd_wire::memory::{MemoryDelta, Visibility};
use tokio::sync::Notify;

use crate::error::Result;
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};
use crate::{InMemory, Memories, MemoryStore};

const WORKSPACE: &str = "01990000-0000-7000-8000-0000000000a1";
const FLEETS: [&str; 3] = [
    "01990000-0000-7000-8000-0000000000b1",
    "01990000-0000-7000-8000-0000000000b2",
    "01990000-0000-7000-8000-0000000000b3",
];
const SOURCE: &str = "source";
const TARGET: &str = "target";
/// The instant every seeded entry was written at.
const SEEDED_AT: i64 = 1_760_000_000_000;
/// A later instant, for the push that races the copy.
const PUSHED_AT: i64 = SEEDED_AT + 60_000;
/// The import the failing target refuses, counted from one.
const FAILS_ON: usize = 5;

fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).expect("a fixture identifier is a v7 spelling")
}

fn delta(key: &str, visibility: Visibility) -> MemoryDelta<'static> {
    MemoryDelta {
        key: Cow::Owned(key.to_owned()),
        content: Cow::Owned(format!("what {key} learned")),
        category: Cow::Borrowed("core"),
        visibility,
    }
}

/// Three fleets, three entries each, two of them shared: nine in all.
async fn seeded(store: &InMemory) {
    let workspace = id(WORKSPACE);
    for (at, fleet) in FLEETS.iter().enumerate() {
        let fleet = id(fleet);
        let owner = Owner {
            workspace: &workspace,
            fleet: &fleet,
        };
        let entries: Vec<_> = (0..3)
            .map(|slot| {
                let shared = at < 2 && slot == 0;
                let visibility = if shared {
                    Visibility::Workspace
                } else {
                    Visibility::Fleet
                };
                delta(&format!("k{at}{slot}"), visibility)
            })
            .collect();
        let refs: Vec<_> = entries.iter().collect();
        store
            .upsert(owner, &refs, UnixMillis::from_millis(SEEDED_AT))
            .await
            .expect("the in-memory store takes a write");
    }
}

/// `store`'s rows, sorted so two stores compare by content.
async fn rows(store: &dyn MemoryStore) -> Vec<Record> {
    let mut rows = store.export(&id(WORKSPACE)).await.expect("an export");
    rows.sort_by(|left, right| (&left.fleet, &left.key).cmp(&(&right.fleet, &right.key)));
    rows
}

/// An in-memory store rigged to fail one import, or to pause its export.
#[derive(Debug)]
struct Rigged {
    inner: InMemory,
    /// The import to refuse, counted from one.
    fails_on: Option<usize>,
    imported: AtomicUsize,
    /// Whether an export snapshots, says so, then waits to be released.
    pauses: bool,
    exported: Notify,
    release: Notify,
}

impl Rigged {
    fn new(name: &'static str, fails_on: Option<usize>, pauses: bool) -> Self {
        Self {
            inner: InMemory::new(name),
            fails_on,
            imported: AtomicUsize::new(0),
            pauses,
            exported: Notify::new(),
            release: Notify::new(),
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

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        let snapshot = self.inner.export(workspace).await;
        if self.pauses {
            self.exported.notify_one();
            self.release.notified().await;
        }
        snapshot
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        let nth = self.imported.fetch_add(1, Ordering::SeqCst) + 1;
        if self.fails_on == Some(nth) {
            return Err(crate::Error::refused(TARGET));
        }
        self.inner.import(workspace, record).await
    }
}

#[tokio::test]
async fn test_flip_copies_every_entry_then_switches() {
    let source = Arc::new(InMemory::new(SOURCE));
    seeded(&source).await;
    let target = Arc::new(InMemory::new(TARGET));
    let memories = Memories::over(
        unreachable_db(),
        Arc::<_>::clone(&source) as Arc<dyn MemoryStore>,
    );

    let flipped = memories
        .flip(
            &id(WORKSPACE),
            Arc::<_>::clone(&target) as Arc<dyn MemoryStore>,
        )
        .await
        .expect("a flip into a store that takes every entry completes");

    assert_eq!(flipped.copied, 9, "every entry of every fleet is copied");
    let copied = rows(target.as_ref()).await;
    assert_eq!(
        copied,
        rows(source.as_ref()).await,
        "writer, key, visibility and instants kept"
    );
    let shared = copied
        .iter()
        .filter(|row| row.visibility.is_workspace())
        .count();
    assert_eq!(shared, 2, "both shared entries stay shared");
    assert_eq!(
        memories.routes().of(&id(WORKSPACE)).store.name(),
        TARGET,
        "the target is the store"
    );
}

#[tokio::test]
async fn test_failed_flip_keeps_the_old_store() {
    let source = Arc::new(InMemory::new(SOURCE));
    seeded(&source).await;
    let before = rows(source.as_ref()).await;
    let target = Arc::new(Rigged::new(TARGET, Some(FAILS_ON), false));
    let memories = Memories::over(
        unreachable_db(),
        Arc::<_>::clone(&source) as Arc<dyn MemoryStore>,
    );

    let refused = memories
        .flip(&id(WORKSPACE), target)
        .await
        .expect_err("a target that refuses entry five fails the flip");

    assert_eq!(refused.code(), afd_core::error_code::MEM_UNAVAILABLE);
    let route = memories.routes().of(&id(WORKSPACE));
    assert_eq!(route.store.name(), SOURCE, "the source is still the store");
    assert!(route.mirror.is_none(), "and no write is mirrored any more");
    assert_eq!(
        rows(source.as_ref()).await,
        before,
        "its nine entries unchanged"
    );
}

#[tokio::test]
async fn test_push_during_flip_reaches_both_stores() {
    let source = Arc::new(Rigged::new(SOURCE, None, true));
    seeded(&source.inner).await;
    let target = Arc::new(InMemory::new(TARGET));
    let memories = Memories::over(
        unreachable_db(),
        Arc::<_>::clone(&source) as Arc<dyn MemoryStore>,
    );
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let raced = delta("k00", Visibility::Workspace);

    let push = async {
        // The copy holds `k00` as seeded in its snapshot when this write lands.
        source.exported.notified().await;
        let owner = Owner {
            workspace: &workspace,
            fleet: &fleet,
        };
        let at = UnixMillis::from_millis(PUSHED_AT);
        memories
            .upsert_through(owner, &[&raced], at)
            .await
            .expect("a push during the copy is taken");
        source.release.notify_one();
    };
    let (flipped, ()) = tokio::join!(
        memories.flip(&workspace, Arc::<_>::clone(&target) as Arc<dyn MemoryStore>),
        push
    );
    flipped.expect("the flip completes around the push");

    // The source read through its inner store: its own export pauses.
    for store in [&source.inner as &dyn MemoryStore, target.as_ref()] {
        let landed = rows(store)
            .await
            .into_iter()
            .find(|row| row.written_by(&fleet) && row.key == "k00")
            .expect("the pushed key is in both stores");
        assert_eq!(
            landed.updated_at_ms,
            PUSHED_AT,
            "{} holds the push, not the snapshot",
            store.name()
        );
    }
}
