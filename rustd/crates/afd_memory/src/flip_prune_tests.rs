//! The prune a flip runs before it copies: a key the fleet forgot does not
//! come back from a failed earlier copy or from a store the workspace lived in
//! before; a rewrite after the prune's read survives its delete, even over rows
//! two store objects share; and one stamped in the very millisecond the prune
//! read comes back with the copy that follows.
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
    FLEETS, Hold, PUSHED_AT, Refuse, Rigged, SEEDED_AT, SOURCE, TARGET, WORKSPACE, delta, id, rows,
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
    let target = Arc::new(Rigged::new(
        TARGET,
        Refuse::Import(REFUSED_IMPORT),
        Hold::Nothing,
    ));
    let memories = over(&source);
    let workspace = id(WORKSPACE);
    let partial = memories.flip(&workspace, shared(&target)).await;
    assert!(partial.is_err(), "precondition: import two is refused");
    let carried = written_at(target.inner.as_ref(), KEY).await;
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
    let kept = written_at(target.inner.as_ref(), KEPT).await;
    assert_eq!(kept, Some(SEEDED_AT), "what the fleet kept moves");
    let forgotten = written_at(target.inner.as_ref(), KEY).await;
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
async fn a_rewrite_in_the_millisecond_the_prune_read_comes_back_with_the_copy() {
    let source = Arc::new(InMemory::new(SOURCE));
    let target = Arc::new(Rigged::new(TARGET, Refuse::Nothing, Hold::ForgetStale));
    // A leftover the source no longer holds, which the prune deletes.
    write(target.inner.as_ref(), KEY, SEEDED_AT).await;
    let memories = over(&source);
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let rewritten = delta(KEY, Visibility::Fleet);
    let entries = [&rewritten];
    // The leftover's own millisecond: the prune's bound takes the rewrite too.
    let at = UnixMillis::from_millis(SEEDED_AT);
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
    let done = flipped.expect("the flip completes");
    let back = Flipped {
        pruned: 1,
        copied: 1,
    };
    assert_eq!(
        done, back,
        "the copy, read after the prune, carries it back"
    );
    let landed = written_at(target.inner.as_ref(), KEY).await;
    assert_eq!(landed, Some(SEEDED_AT), "the new store holds the write");
}

#[tokio::test]
async fn a_rewrite_after_the_prunes_read_survives_over_rows_two_stores_share() {
    let rows_shared = Arc::new(InMemory::new(SOURCE));
    write(rows_shared.as_ref(), KEY, SEEDED_AT).await;
    // One row set, two identities: `to` holds the prune's first read, `from`
    // its second.
    let from = Arc::new(Rigged::sharing(&rows_shared, Refuse::Nothing, Hold::Export));
    let to = Arc::new(Rigged::sharing(&rows_shared, Refuse::Nothing, Hold::Export));
    let memories = over(&from);
    let (workspace, fleet) = (id(WORKSPACE), id(FLEETS[0]));
    let rewritten = delta(KEY, Visibility::Fleet);
    let entries = [&rewritten];
    let at = UnixMillis::from_millis(PUSHED_AT);
    let interleave = async {
        // `to` was read holding `KEY`; it goes before `from` is read.
        let read_to = tokio::time::timeout(PATIENCE, to.reached.notified()).await;
        forget(rows_shared.as_ref(), KEY).await;
        to.release.notify_one();
        // `from` was read without it; the fleet writes it again before the
        // prune deletes.
        let read_from = tokio::time::timeout(PATIENCE, from.reached.notified()).await;
        let owner = owner(&workspace, &fleet);
        let written = memories.upsert_through(owner, &entries, at).await;
        from.release.notify_one();
        (read_to.is_ok() && read_from.is_ok(), written)
    };

    let flip = memories.flip(&workspace, shared(&to));
    let (flipped, (read, written)) = tokio::join!(flip, interleave);

    assert!(read, "precondition: the prune reads `to`, then `from`");
    written.expect("the rewrite is taken");
    let done = flipped.expect("a flip between two objects over one store completes");
    let spared = Flipped {
        pruned: 0,
        copied: 1,
    };
    assert_eq!(done, spared, "the prune spares the rewrite");
    let survived = written_at(rows_shared.as_ref(), KEY).await;
    assert_eq!(survived, Some(PUSHED_AT), "the shared rows still hold it");
}
