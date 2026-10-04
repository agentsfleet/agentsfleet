//! A flip's prune against the live store: its bounded delete spares a row newer
//! than the version it read and takes one at or under it, and across two store
//! objects over one database, a rewrite after the prune's read survives the
//! flip.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_memory::{Flipped, Memories, MemoryStore, Owner, PgStore};
use afd_wire::memory::{MemoryDelta, PINNED_CATEGORY, Visibility};

use crate::paused::{Paused, Verb};
use crate::workspace::{Grants, Workspace, delta};

/// The key the prune reads, and the instant it was first written at.
const KEY: &str = "deploy_target";
const SEEDED_AT: i64 = 1_760_000_000_000;
/// A second key, under the bound it is forgotten with.
const OLDER: &str = "older";
/// The later instant of the rewrite the prune must spare.
const REWRITTEN_AT: i64 = SEEDED_AT + 60_000;

fn owner<'a>(space: &'a Workspace, fleet: &'a Uuid7) -> Owner<'a> {
    Owner {
        workspace: &space.id,
        fleet,
    }
}

/// A Postgres store object over the lane database: a new identity each call,
/// the same rows.
fn postgres(space: &Workspace) -> PgStore {
    PgStore::new(space.database.clone(), Entropy::new())
}

/// Writes `entries` as `fleet` into `store` at the seeded instant.
async fn seed(store: &PgStore, space: &Workspace, fleet: &Uuid7, entries: &[MemoryDelta<'_>]) {
    let refs: Vec<_> = entries.iter().collect();
    let at = UnixMillis::from_millis(SEEDED_AT);
    let seeded = store.upsert(owner(space, fleet), &refs, at).await;
    seeded.expect("a seed");
}

/// Every `(key, updated_at)` `store` holds in `space`.
async fn versions(store: &dyn MemoryStore, space: &Workspace) -> Vec<(String, i64)> {
    let rows = store.export(&space.id).await.expect("an export");
    rows.into_iter()
        .map(|row| (row.key, row.updated_at_ms))
        .collect()
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_stale_forget_spares_a_newer_row_and_takes_one_at_or_under_its_bound() {
    let space = Workspace::create().await;
    let fleet = space.fleet("pruned", Grants::default()).await;
    let store = postgres(&space);
    let seeds = [
        delta(KEY, PINNED_CATEGORY, Visibility::Fleet),
        delta(OLDER, PINNED_CATEGORY, Visibility::Fleet),
    ];
    seed(&store, &space, &fleet, &seeds).await;
    let forget = |key, bound| store.forget_stale(owner(&space, &fleet), key, bound);

    let removed = [
        forget(KEY, SEEDED_AT - 1).await,
        forget(KEY, SEEDED_AT).await,
        forget(OLDER, SEEDED_AT + 1).await,
    ]
    .map(|answer| answer.expect("a stale forget"));

    let expected = [false, true, true];
    assert_eq!(removed, expected, "newer stays; at the bound and under go");
    let left = versions(&store, &space).await;
    assert!(left.is_empty(), "both rows went: {left:?}");
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_rewrite_after_the_prunes_read_survives_two_stores_over_one_database() {
    let space = Workspace::create().await;
    let fleet = space.fleet("rewriter", Grants::default()).await;
    let table = postgres(&space);
    let held = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    seed(&table, &space, &fleet, &held).await;
    // Two store objects over the table: `from` holds the prune's read of it,
    // `to` the prune's delete.
    let from = Arc::new(Paused::over(postgres(&space), Verb::Export));
    let to = Arc::new(Paused::over(postgres(&space), Verb::ForgetStale));
    let memories = Memories::over(space.database.clone(), Arc::clone(&from) as _);
    let interleave = async {
        // `to` was read holding `KEY`; it goes before `from` is read.
        from.reached.notified().await;
        let gone = table.forget(owner(&space, &fleet), KEY).await;
        assert!(gone.expect("a removal"), "the seed was there to remove");
        from.release.notify_one();
        // `from` was read without it; the fleet writes it again before the
        // prune deletes.
        to.reached.notified().await;
        let later = UnixMillis::from_millis(REWRITTEN_AT);
        let captured = memories.capture(&fleet, &held, later).await;
        to.release.notify_one();
        captured
    };

    let flip = memories.flip(&space.id, Arc::clone(&to) as _);
    let (flipped, captured) = tokio::join!(flip, interleave);

    assert_eq!(captured.expect("the rewrite is taken").stored, 1);
    let done = flipped.expect("a flip between two objects over one table completes");
    let spared = Flipped {
        pruned: 0,
        copied: 1,
    };
    assert_eq!(done, spared, "the prune spares the rewrite");
    let survived = versions(&table, &space).await;
    assert_eq!(
        survived,
        [(KEY.to_owned(), REWRITTEN_AT)],
        "it is in the table"
    );
    space.cleanup().await;
}
