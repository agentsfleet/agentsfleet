//! Dimension 4.1: shared memory reaches only granted fleets, each entry naming
//! its writer — through hydrate, recall and the operator page alike.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_memory::Record;
use afd_memory::page::{After, View};
use afd_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use afd_wire::memory::{PINNED_CATEGORY, Visibility};

use crate::workspace::{Grants, Workspace, delta};

/// The publishing fleet's name, which a reader sees on every shared entry.
const PUBLISHER: &str = "incident-fleet-3";
/// The key the publisher shares.
const SHARED_KEY: &str = "deploy_target";
/// How many entries a recall asks for.
const LIMIT: usize = 5;
/// The text a search page looks for, which the shared key holds.
const SEARCHED: &str = "deploy";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_shared_memory_reaches_only_granted_fleets() {
    let space = Workspace::create().await;
    let publish = Grants {
        publish: true,
        ..Grants::default()
    };
    let read = Grants {
        read: true,
        ..Grants::default()
    };
    let writer = space.fleet(PUBLISHER, publish).await;
    let reader = space.fleet("new-fleet", read).await;
    let stranger = space.fleet("private-fleet", Grants::default()).await;
    let published = [
        delta(SHARED_KEY, PINNED_CATEGORY, Visibility::Workspace),
        delta("own-notes", PINNED_CATEGORY, Visibility::Fleet),
    ];
    space
        .memories
        .capture(
            &writer,
            &published,
            UnixMillis::from_millis(1_760_000_000_000),
        )
        .await
        .expect("the publisher's capture");

    let granted = space
        .memories
        .hydrate(&reader)
        .await
        .expect("the reader's hydrate");
    assert!(
        granted.memory.is_empty(),
        "the reader holds nothing of its own"
    );
    assert_eq!(
        granted.shared.len(),
        1,
        "only the shared entry, never the private one"
    );
    let shared = &granted.shared[0];
    assert_eq!(shared.key, SHARED_KEY);
    assert_eq!(shared.writer_fleet_name, PUBLISHER);
    assert_eq!(shared.writer_fleet_id, writer.as_str());

    let ungranted = space
        .memories
        .hydrate(&stranger)
        .await
        .expect("the stranger's hydrate");
    assert!(
        ungranted.shared.is_empty(),
        "no read grant, no shared entry"
    );
    let own_view = space
        .memories
        .hydrate(&writer)
        .await
        .expect("the writer's hydrate");
    assert!(
        own_view.publish && own_view.shared.is_empty(),
        "a writer reads only its own"
    );

    let recalled = space
        .memories
        .recall(&reader, "deploy", LIMIT)
        .await
        .expect("a recall");
    assert_eq!(recalled.shared.len(), 1);
    assert_eq!(recalled.shared[0].writer_fleet_name, PUBLISHER);
    let refused = space
        .memories
        .recall(&stranger, "deploy", LIMIT)
        .await
        .expect("a recall");
    assert!(refused.shared.is_empty() && refused.memory.is_empty());

    let page = space
        .memories
        .page(
            &space.id,
            &reader,
            View::Recent,
            None,
            i64::try_from(LIMIT).unwrap_or(1),
        )
        .await
        .expect("the reader's page");
    assert_eq!(page.len(), 1);
    assert!(page[0].written_by(&writer) && page[0].visibility.is_workspace());
    space.cleanup().await;
}

/// Two publishers share one key in one millisecond; a reader paging `view`
/// one row at a time reaches both, because the writer is part of the keyset.
///
/// Each view runs its own continuation statement, so each has its own chance
/// to drop the writer from the seek; the tests below walk every one.
async fn a_page_walk_past_two_tied_writers_skips_neither(view: View<'_>) {
    let space = Workspace::create().await;
    let publish = Grants {
        publish: true,
        ..Grants::default()
    };
    let read = Grants {
        read: true,
        ..Grants::default()
    };
    let first = space.fleet("publisher-a", publish).await;
    let second = space.fleet("publisher-b", publish).await;
    let reader = space.fleet("reader", read).await;
    let shared = [delta(SHARED_KEY, PINNED_CATEGORY, Visibility::Workspace)];
    let tied = UnixMillis::from_millis(1_760_000_000_000);
    for writer in [&first, &second] {
        space
            .memories
            .capture(writer, &shared, tied)
            .await
            .expect("a publish");
    }

    let mut walked: Vec<Record> = Vec::new();
    // One page per writer, and one more that must come back empty.
    for _page in 0..3 {
        let boundary = walked.last().map(|row| After {
            created_at_ms: row.created_at_ms,
            key: &row.key,
            fleet: &row.fleet,
        });
        let page = space
            .memories
            .page(&space.id, &reader, view, boundary, 1)
            .await
            .expect("a page");
        walked.extend(page);
    }

    let mut writers: Vec<_> = walked.iter().map(|row| row.fleet.clone()).collect();
    assert_eq!(writers.len(), 2, "both writers' entries, neither repeated");
    writers.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(writers, expected);
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_page_walk_past_two_writers_tied_on_instant_and_key_skips_neither() {
    a_page_walk_past_two_tied_writers_skips_neither(View::Recent).await;
}

/// The two tied rows sit in one category, so the category walk meets the
/// same tie the recent walk does.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_category_page_walk_past_two_writers_tied_on_instant_and_key_skips_neither() {
    a_page_walk_past_two_tied_writers_skips_neither(View::Category(PINNED_CATEGORY)).await;
}

/// Both tied rows hold the searched text in their key, so the search walk
/// meets the tie too.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_search_page_walk_past_two_writers_tied_on_instant_and_key_skips_neither() {
    a_page_walk_past_two_tied_writers_skips_neither(View::Search(SEARCHED)).await;
}

/// A change naming neither grant writes nothing and answers both as they are.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_access_change_naming_neither_grant_answers_both_as_they_stand() {
    let space = Workspace::create().await;
    let read = Grants {
        read: true,
        ..Grants::default()
    };
    let reader = space.fleet("reader", read).await;

    let access = space
        .memories
        .set_access(&space.id, &reader, MemoryAccessRequest::default())
        .await
        .expect("an access change naming neither grant");

    let held = MemoryAccess {
        read: true,
        publish: false,
    };
    assert_eq!(access, held, "the grants the fleet was seeded with");
    space.cleanup().await;
}
