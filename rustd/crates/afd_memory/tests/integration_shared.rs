//! Dimension 4.1: shared memory reaches only granted fleets, each entry naming
//! its writer — through hydrate, recall and the operator page alike.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_core::clock::UnixMillis;
use afd_memory::page::View;
use afd_wire::memory::{PINNED_CATEGORY, Visibility};

use crate::workspace::{Grants, Workspace, delta};

/// The publishing fleet's name, which a reader sees on every shared entry.
const PUBLISHER: &str = "incident-fleet-3";
/// The key the publisher shares.
const SHARED_KEY: &str = "deploy_target";
/// How many entries a recall asks for.
const LIMIT: usize = 5;

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
