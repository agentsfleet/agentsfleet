//! A flip against the real grants: into Postgres with every entry kept as it
//! stands, a retry into Postgres that prunes what a failed copy left, the
//! capture and forgets that race a flip's copy, and the forget and flip a
//! dropped flip leaves room for.
//!
//! The grants are read from `core.fleets`, so these need the live schema; the
//! racing stores are in memory, so the test lands the flip where it wants.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::error_code::MEM_UNAVAILABLE;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::test_util::mint_id;
use afd_memory::error::detail::MOVING;
use afd_memory::{Flipped, InMemory, Memories, MemoryStore, Owner, PgStore, Record};
use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY, Visibility};

use crate::paused::{Paused, Verb};
use crate::workspace::{Grants, Workspace, delta};

/// The store a flip leaves, and the one it moves to.
const SOURCE: &str = "source";
const TARGET: &str = "target";
/// The name the Postgres store gives the flip's log lines.
const POSTGRES: &str = "postgres";
/// The key every racing call is about.
const KEY: &str = "deploy_target";
/// A second key, in the category neither pinned nor swept.
const CHAT: &str = "chat";
const CONVERSATION: &str = "conversation";
/// The key a writer `core.fleets` never held stores, which Postgres refuses.
const ORPHAN: &str = "orphan";
/// The fleet whose forget a flip meets.
const FORGETTER: &str = "forgetter";
/// The instant every seeded entry was first written at.
const AT: i64 = 1_760_000_000_000;

fn owner<'a>(space: &'a Workspace, fleet: &'a Uuid7) -> Owner<'a> {
    Owner {
        workspace: &space.id,
        fleet,
    }
}

/// Writes `entries` under `owner` straight into `store` at `at`.
async fn seed(store: &InMemory, owner: Owner<'_>, entries: &[MemoryDelta<'_>], at: i64) {
    let refs: Vec<_> = entries.iter().collect();
    store
        .upsert(owner, &refs, UnixMillis::from_millis(at))
        .await
        .expect("a seed");
}

/// Every row `store` holds in `space`.
async fn exported(store: &dyn MemoryStore, space: &Workspace) -> Vec<Record> {
    store.export(&space.id).await.expect("an export")
}

/// `exported`, in `(writer, key)` order, so two stores compare by content.
async fn sorted(store: &dyn MemoryStore, space: &Workspace) -> Vec<Record> {
    let mut rows = exported(store, space).await;
    rows.sort_by(|left, right| (&left.fleet, &left.key).cmp(&(&right.fleet, &right.key)));
    rows
}

/// Every key `store` holds in `space`.
async fn keys(store: &dyn MemoryStore, space: &Workspace) -> Vec<String> {
    let rows = exported(store, space).await;
    rows.into_iter().map(|row| row.key).collect()
}

fn completed(flipped: afd_memory::Result<Flipped>) -> Flipped {
    flipped.expect("the flip completes")
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_flip_into_postgres_writes_every_entry_as_it_stands() {
    let space = Workspace::create().await;
    let fleet = space.fleet("mover", Grants::default()).await;
    let source = Arc::new(InMemory::new(SOURCE));
    let first = [
        delta(KEY, PINNED_CATEGORY, Visibility::Workspace),
        delta(CHAT, CONVERSATION, Visibility::Fleet),
    ];
    seed(&source, owner(&space, &fleet), &first, AT).await;
    // A second write moves `chat`'s last instant past its first.
    let again = [delta(CHAT, CONVERSATION, Visibility::Fleet)];
    seed(&source, owner(&space, &fleet), &again, AT + 1).await;
    let postgres = Arc::new(PgStore::new(space.database.clone(), Entropy::new()));
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);

    let flipped = memories.flip(&space.id, Arc::clone(&postgres) as _).await;

    assert_eq!(completed(flipped).copied, 2, "every entry is copied");
    assert_eq!(
        sorted(postgres.as_ref(), &space).await,
        sorted(source.as_ref(), &space).await,
        "writer, key, content, category, visibility and both instants kept"
    );
    assert_eq!(postgres.name(), POSTGRES, "the name the flip logs");
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_retried_flip_into_postgres_does_not_bring_back_a_forgotten_key() {
    let space = Workspace::create().await;
    let fleet = space.fleet(FORGETTER, Grants::default()).await;
    let ghost = Uuid7::parse(&mint_id()).expect("a minted id is canonical");
    let source = Arc::new(InMemory::new(SOURCE));
    let kept = [delta(CHAT, CONVERSATION, Visibility::Fleet)];
    seed(&source, owner(&space, &fleet), &kept, AT).await;
    let orphan = [delta(ORPHAN, CONVERSATION, Visibility::Fleet)];
    seed(&source, owner(&space, &ghost), &orphan, AT + 1).await;
    // Newest, so exported and imported first, before the orphan is refused.
    let doomed = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    seed(&source, owner(&space, &fleet), &doomed, AT + 2).await;
    let postgres = Arc::new(PgStore::new(space.database.clone(), Entropy::new()));
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);
    let partial = memories.flip(&space.id, Arc::clone(&postgres) as _).await;
    assert!(
        partial.is_err(),
        "precondition: Postgres refuses the orphan"
    );
    let carried = keys(postgres.as_ref(), &space).await;
    assert_eq!(carried, [KEY], "after taking the newest entry");
    let forgotten = memories.forget(&space.id, &fleet, KEY).await;
    forgotten.expect("the fleet forgets on the store it stayed on");
    let gone = source.forget(owner(&space, &ghost), ORPHAN).await;
    assert!(
        gone.expect("an in-memory forget"),
        "so the retry can finish"
    );

    let retried = memories.flip(&space.id, Arc::clone(&postgres) as _).await;

    assert_eq!(
        completed(retried),
        Flipped {
            pruned: 1,
            copied: 1
        }
    );
    let left = keys(postgres.as_ref(), &space).await;
    assert_eq!(left, [CHAT], "the forgotten key stays forgotten");
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_forget_while_a_flip_copies_is_told_to_try_again() {
    let space = Workspace::create().await;
    let fleet = space.fleet(FORGETTER, Grants::default()).await;
    let source = Arc::new(Paused::new(SOURCE, Verb::Export));
    let held = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    seed(&source.inner, owner(&space, &fleet), &held, AT).await;
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);
    let target = Arc::new(InMemory::new(TARGET));
    let forget = async {
        source.reached.notified().await;
        let refused = memories.forget(&space.id, &fleet, KEY).await;
        source.release.notify_one();
        refused
    };

    let (flipped, refused) =
        tokio::join!(memories.flip(&space.id, Arc::clone(&target) as _), forget);

    let refused = refused.expect_err("a forget while the copy runs is refused");
    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_UNAVAILABLE, MOVING)
    );
    assert_eq!(completed(flipped).copied, 1);
    assert_eq!(
        keys(target.as_ref(), &space).await,
        [KEY],
        "the entry moved"
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_forget_that_a_flip_overtook_is_told_to_try_again() {
    let space = Workspace::create().await;
    let fleet = space.fleet(FORGETTER, Grants::default()).await;
    let source = Arc::new(Paused::new(SOURCE, Verb::Forget));
    let held = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    seed(&source.inner, owner(&space, &fleet), &held, AT).await;
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);
    let target = Arc::new(InMemory::new(TARGET));
    let flip = async {
        source.reached.notified().await;
        let flipped = memories.flip(&space.id, Arc::clone(&target) as _).await;
        source.release.notify_one();
        flipped
    };

    let (refused, flipped) = tokio::join!(memories.forget(&space.id, &fleet, KEY), flip);

    let refused = refused.expect_err("a forget the flip overtook is refused");
    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_UNAVAILABLE, MOVING)
    );
    assert_eq!(completed(flipped).copied, 1);
    assert_eq!(
        keys(target.as_ref(), &space).await,
        [KEY],
        "the copy carried the entry before the forget reached the old store"
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_capture_that_straddles_a_flip_lands_in_the_new_store() {
    let space = Workspace::create().await;
    let fleet = space.fleet("pusher", Grants::default()).await;
    let source = Arc::new(Paused::new(SOURCE, Verb::Upsert));
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);
    let target = Arc::new(InMemory::new(TARGET));
    let pushed = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    let flip = async {
        source.reached.notified().await;
        let flipped = memories.flip(&space.id, Arc::clone(&target) as _).await;
        source.release.notify_one();
        flipped
    };

    let at = UnixMillis::from_millis(AT);
    let (captured, flipped) = tokio::join!(memories.capture(&fleet, &pushed, at), flip);

    assert_eq!(captured.expect("the capture is taken").stored, 1);
    let copied = completed(flipped).copied;
    assert_eq!(copied, 0, "the copy ran before the capture wrote anything");
    assert_eq!(
        keys(target.as_ref(), &space).await,
        [KEY],
        "the capture reached the store the flip moved to"
    );
    assert_eq!(
        keys(&source.inner, &space).await,
        [KEY],
        "and the one it left"
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_flip_dropped_before_it_switches_takes_the_next_forget_and_flip() {
    let space = Workspace::create().await;
    let fleet = space.fleet("dropper", Grants::default()).await;
    let source = Arc::new(Paused::new(SOURCE, Verb::Export));
    let seeds = [
        delta(KEY, PINNED_CATEGORY, Visibility::Fleet),
        delta(CHAT, CONVERSATION, Visibility::Fleet),
    ];
    seed(&source.inner, owner(&space, &fleet), &seeds, AT).await;
    let memories = Memories::over(space.database.clone(), Arc::clone(&source) as _);
    let mut flip = Box::pin(memories.flip(&space.id, Arc::new(InMemory::new(TARGET))));
    let answered = tokio::select! {
        biased;
        flipped = &mut flip => Some(flipped),
        () = source.reached.notified() => None,
    };
    assert!(answered.is_none(), "the flip is held in its export");

    drop(flip);

    let forgotten = memories.forget(&space.id, &fleet, KEY).await;
    forgotten.expect("a forget after the dropped flip is taken");
    let target = Arc::new(InMemory::new(TARGET));
    let flipped = memories.flip(&space.id, Arc::clone(&target) as _).await;
    assert_eq!(flipped.expect("the next flip is taken").copied, 1);
    assert_eq!(
        keys(target.as_ref(), &space).await,
        [CHAT],
        "only what the forget left"
    );
    space.cleanup().await;
}
