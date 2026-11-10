//! The lease path's per-replica state follows every change that matters to a
//! run (M245 §3): what is cached is keyed by a version a statement in the same
//! pass read, and what is not cached rides the claim.
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use std::borrow::Cow;

use af_wire::memory::{MemoryDelta, PINNED_CATEGORY, Visibility};
use afd_api_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use afd_fleet::lease::Acquired;

use crate::rows::{LAPSE, Row};
use crate::seed::{seeded_parts, select_fleet};
use crate::support::Fixtures;

/// The key the publishing fleet shares with its workspace.
const SHARED_KEY: &str = "deploy_target";

/// The window a lease carries, built as the lease path builds it: from the
/// workspace and grants its claim statement read.
async fn window_of(
    memories: &afd_memory::Memories,
    acquired: &Acquired,
) -> af_wire::memory::MemoryHydrateResponse<'static> {
    let grants = MemoryAccess {
        read: acquired.opened.memory_reads_workspace,
        publish: acquired.opened.memory_publishes_workspace,
    };
    memories
        .hydrate_scoped(&acquired.fleet_id, &acquired.workspace_id, grants)
        .await
        .expect("the window reads")
}

// Value: protects=a grant change reaches the next lease; fails_when=the scope
// is cached or read before the claim; why_new=the scope moved into the claim
// statement; seam=none; no_claim=a change during a running lease.
#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn memory_scope_follows_grant_change() {
    let fixtures = Fixtures::create().await;
    let (reader, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let writer = afd_db::test_util::mint_id();
    fixtures
        .seed_fleet(
            &writer,
            &workspace,
            &tenant,
            &crate::requests::placement_tag(&writer),
        )
        .await;
    let memories = fixtures.plane().memories;
    memories
        .set_access(
            &workspace,
            &writer,
            MemoryAccessRequest {
                read: None,
                publish: Some(true),
            },
        )
        .await
        .expect("the writer may publish");
    let shared = [MemoryDelta {
        key: Cow::Borrowed(SHARED_KEY),
        content: Cow::Borrowed("the staging cluster"),
        category: Cow::Borrowed(PINNED_CATEGORY),
        visibility: Visibility::Workspace,
    }];
    memories
        .capture(&writer, &shared, fixtures.enrolled_at())
        .await
        .expect("the writer shares an entry");
    crate::queue::enqueue(
        &fixtures.database,
        &reader,
        &workspace,
        "steer:user_1",
        "chat",
        r#"{"message":"where do we deploy?"}"#,
        fixtures.enrolled_at(),
    )
    .await;
    let leases = fixtures.leases_with_dead_bus();

    let first = select_fleet(&leases, &runner, &reader)
        .await
        .expect("the reader's first lease");
    assert!(!first.opened.memory_reads_workspace);
    assert!(
        window_of(&memories, &first).await.shared.is_empty(),
        "no read grant, no shared entry"
    );

    memories
        .set_access(
            &workspace,
            &reader,
            MemoryAccessRequest {
                read: Some(true),
                publish: None,
            },
        )
        .await
        .expect("the reader is granted workspace memory");
    fixtures
        .age(Row::Slot(&reader), "lease_expires_at", LAPSE)
        .await;
    let second = select_fleet(&leases, &runner, &reader)
        .await
        .expect("the reader's next lease");
    assert!(
        second.opened.memory_reads_workspace,
        "the claim read the grant"
    );
    let window = window_of(&memories, &second).await;
    assert_eq!(
        window.shared.len(),
        1,
        "the granted lease reads the shared entry"
    );
    assert_eq!(
        window.shared.first().map(|entry| entry.key.as_ref()),
        Some(SHARED_KEY)
    );
    fixtures.cleanup().await;
}
