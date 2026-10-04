//! The prune a flip runs before it copies: a key the fleet forgot does not
//! come back from a failed earlier copy or from a store the workspace lived in
//! before, and a write that lands while the prune deletes its key survives.
#![expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_db::test_util::unreachable_db;
use afd_wire::memory::Visibility;

use super::{
    FLEETS, Hold, PUSHED_AT, Rigged, SEEDED_AT, SOURCE, TARGET, WORKSPACE, delta, id, rows,
};
use crate::record::Owner;
use crate::{Flipped, InMemory, Memories, MemoryStore};

/// The key the fleet forgets, or writes again while the prune deletes it.
const KEY: &str = "forgotten";
/// A key the fleet keeps throughout.
const KEPT: &str = "kept";
/// The import a failing target refuses: the second, after `KEY` landed.
const REFUSED_IMPORT: usize = 2;
/// How long the racing write waits for the prune to reach its delete.
const PATIENCE: Duration = Duration::from_secs(5);

/// `store` as the trait object a flip takes.
fn shared<S: MemoryStore + 'static>(store: &Arc<S>) -> Arc<dyn MemoryStore> {
    Arc::<S>::clone(store)
}

/// A table over `store`, with no grants to read.
fn over<S: MemoryStore + 'static>(store: &Arc<S>) -> Memories {
    Memories::over(unreachable_db(), shared(store))
}

fn owner<'a>(workspace: &'a Uuid7, fleet: &'a Uuid7) -> Owner<'a> {
    Owner { workspace, fleet }
}

/// The first fleet writes `key` into `store` at `at`.
async fn write(store: &dyn MemoryStore, key: &str, at: i64) {
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let entry = delta(key, Visibility::Fleet);
    store
        .upsert(
            owner(&workspace, &fleet),
            &[&entry],
            UnixMillis::from_millis(at),
        )
        .await
        .expect("the in-memory store takes a write");
}

/// The first fleet forgets `key` in `store`, which held it.
async fn forget(store: &dyn MemoryStore, key: &str) {
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let forgotten = store.forget(owner(&workspace, &fleet), key).await;
    assert!(
        forgotten.expect("the in-memory store forgets"),
        "it held {key}"
    );
}

/// When `store` holds `key`, the instant it was last written.
async fn written_at(store: &dyn MemoryStore, key: &str) -> Option<i64> {
    let held = rows(store).await;
    held.into_iter()
        .find(|row| row.key == key)
        .map(|row| row.updated_at_ms)
}

#[tokio::test]
async fn a_retried_flip_does_not_bring_back_a_key_forgotten_after_a_partial_copy() {
    let source = Arc::new(InMemory::new(SOURCE));
    write(source.as_ref(), KEPT, SEEDED_AT).await;
    // The newer entry is exported, and imported, first.
    write(source.as_ref(), KEY, PUSHED_AT).await;
    let target = Arc::new(Rigged::new(TARGET, Some(REFUSED_IMPORT), Hold::Nothing));
    let memories = over(&source);
    let workspace = id(WORKSPACE);
    let partial = memories.flip(&workspace, shared(&target)).await;
    assert!(partial.is_err(), "precondition: import two is refused");
    let carried = written_at(&target.inner, KEY).await;
    assert_eq!(carried, Some(PUSHED_AT), "after import one landed");
    forget(source.as_ref(), KEY).await;

    let retried = memories.flip(&workspace, shared(&target)).await;

    let done = retried.expect("the retry completes");
    assert_eq!(
        done,
        Flipped {
            pruned: 1,
            copied: 1
        }
    );
    let kept = written_at(&target.inner, KEPT).await;
    assert_eq!(kept, Some(SEEDED_AT), "what the fleet kept moves");
    let forgotten = written_at(&target.inner, KEY).await;
    assert_eq!(forgotten, None, "the forgotten key stays forgotten");
}

#[tokio::test]
async fn a_flip_back_does_not_bring_back_a_key_forgotten_while_away() {
    let first = Arc::new(InMemory::new(SOURCE));
    write(first.as_ref(), KEY, SEEDED_AT).await;
    let second = Arc::new(InMemory::new(TARGET));
    let memories = over(&first);
    let workspace = id(WORKSPACE);
    let away = memories.flip(&workspace, shared(&second)).await;
    assert_eq!(away.expect("the flip away completes").copied, 1);
    forget(second.as_ref(), KEY).await;

    let back = memories.flip(&workspace, shared(&first)).await;

    let done = back.expect("the flip back completes");
    assert_eq!(
        done,
        Flipped {
            pruned: 1,
            copied: 0
        }
    );
    let left = written_at(first.as_ref(), KEY).await;
    assert_eq!(left, None, "the old store's copy does not come back");
}

#[tokio::test]
async fn a_write_landing_while_the_prune_deletes_its_key_survives_the_switch() {
    let source = Arc::new(InMemory::new(SOURCE));
    let target = Arc::new(Rigged::new(TARGET, None, Hold::Forget));
    // A leftover the source no longer holds, which the prune deletes.
    write(&target.inner, KEY, SEEDED_AT).await;
    let memories = over(&source);
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let rewritten = delta(KEY, Visibility::Fleet);
    let entries = [&rewritten];
    let at = UnixMillis::from_millis(PUSHED_AT);
    let racing = async {
        // The prune has read both stores, and holds its delete of `KEY`.
        let reached = tokio::time::timeout(PATIENCE, target.reached.notified()).await;
        let owner = owner(&workspace, &fleet);
        let written = memories.upsert_through(owner, &entries, at).await;
        target.release.notify_one();
        (reached.is_ok(), written)
    };

    let flip = memories.flip(&workspace, shared(&target));
    let (flipped, (reached, written)) = tokio::join!(flip, racing);

    assert!(reached, "precondition: the prune deletes the leftover");
    written.expect("the write is taken, both halves");
    assert_eq!(flipped.expect("the flip completes").pruned, 1);
    let landed = written_at(&target.inner, KEY).await;
    assert_eq!(landed, Some(PUSHED_AT), "the new store holds the write");
}
