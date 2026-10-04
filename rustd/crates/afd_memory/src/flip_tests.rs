//! Dimensions 2.1–2.3: a flip copies every entry, a flip onto the store it is
//! on keeps them all, a failed copy keeps the old store, and a push racing the
//! copy reaches both stores.
//!
//! Against the in-memory store, so the interleaving is the test's to force: a
//! store that holds one verb pins the flip at the exact point a push has to
//! survive.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;
use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code::MEM_UNAVAILABLE;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_db::test_util::unreachable_db;
use afd_wire::memory::{MemoryDelta, Visibility};

use self::fixture::{Hold, Rigged};
use super::EVENT_FAILED;
use crate::record::{Owner, Record};
use crate::{Flipped, InMemory, Memories, MemoryStore};

#[path = "flip_fixture.rs"]
mod fixture;
#[path = "flip_prune_tests.rs"]
mod prune;
#[path = "flip_race_tests.rs"]
mod race;

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
/// The field a failure's log line carries its registry code in.
const ERROR_CODE: &str = "error_code";

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
async fn test_a_flip_onto_the_store_it_is_on_keeps_every_entry() {
    let source = Arc::new(InMemory::new(SOURCE));
    seeded(&source).await;
    let before = rows(source.as_ref()).await;
    let store = Arc::<_>::clone(&source) as Arc<dyn MemoryStore>;
    let memories = Memories::over(unreachable_db(), Arc::clone(&store));

    let flipped = memories
        .flip(&id(WORKSPACE), Arc::clone(&store))
        .await
        .expect("a flip onto the store it is on completes");

    assert_eq!(
        flipped,
        Flipped {
            pruned: 0,
            copied: 0
        },
        "nothing is pruned from or copied onto the store it is on"
    );
    assert_eq!(rows(source.as_ref()).await, before, "and kept as it stood");
    let route = memories.routes().of(&id(WORKSPACE));
    assert!(Arc::ptr_eq(&route.store, &store), "on the store it was on");
    assert!(route.mirror.is_none(), "and no write is mirrored");
}

#[tokio::test]
async fn test_failed_flip_keeps_the_old_store() {
    let source = Arc::new(InMemory::new(SOURCE));
    seeded(&source).await;
    let before = rows(source.as_ref()).await;
    let target = Arc::new(Rigged::new(TARGET, Some(FAILS_ON), Hold::Nothing));
    let memories = Memories::over(
        unreachable_db(),
        Arc::<_>::clone(&source) as Arc<dyn MemoryStore>,
    );

    let capture = Capture::install();

    let refused = memories
        .flip(&id(WORKSPACE), target)
        .await
        .expect_err("a target that refuses entry five fails the flip");

    assert_eq!(refused.code(), MEM_UNAVAILABLE);
    let failed = capture.only(EVENT_FAILED);
    let logged = failed.field(ERROR_CODE);
    assert_eq!(
        logged,
        Some(MEM_UNAVAILABLE.as_str()),
        "logged once, as refused"
    );
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
    let source = Arc::new(InMemory::new(SOURCE));
    seeded(&source).await;
    // Held at its first import, by when the copy has its snapshot.
    let target = Arc::new(Rigged::new(TARGET, None, Hold::Import));
    let memories = Memories::over(
        unreachable_db(),
        Arc::<_>::clone(&source) as Arc<dyn MemoryStore>,
    );
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let raced = delta("k00", Visibility::Workspace);

    let push = async {
        // The copy holds `k00` as seeded in its snapshot when this write lands.
        target.reached.notified().await;
        let owner = Owner {
            workspace: &workspace,
            fleet: &fleet,
        };
        let at = UnixMillis::from_millis(PUSHED_AT);
        memories
            .upsert_through(owner, &[&raced], at)
            .await
            .expect("a push during the copy is taken");
        target.release.notify_one();
    };
    let (flipped, ()) = tokio::join!(
        memories.flip(&workspace, Arc::<_>::clone(&target) as Arc<dyn MemoryStore>),
        push
    );
    flipped.expect("the flip completes around the push");

    // The target read through its inner store, past its hold.
    for store in [source.as_ref() as &dyn MemoryStore, &target.inner] {
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
