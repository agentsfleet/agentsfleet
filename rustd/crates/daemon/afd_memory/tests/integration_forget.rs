//! The operator's forget and grant change, and the one refusal a fleet outside
//! the named workspace gets from every operator verb.
#![expect(
    clippy::expect_used,
    reason = "integration test: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::fleet::{MemoryAccess, MemoryAccessRequest};
use afd_core::error_code::{MEM_AGENTSFLEET_NOT_FOUND, MEM_ENTRY_NOT_FOUND};
use afd_db::test_util::mint_uuid7;
use afd_memory::error::detail::{ENTRY_NOT_FOUND, FLEET_NOT_FOUND};
use afd_memory::page::View;
use afd_wire::memory::{PINNED_CATEGORY, Visibility};

use crate::workspace::{Grants, Workspace, delta};

/// The instant every capture here is stamped with.
const AT: jiff::Timestamp = jiff::Timestamp::constant(1_760_000_000, 0);
/// The key both fleets write.
const KEY: &str = "deploy_target";

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_forget_removes_the_fleets_own_entry_and_never_anothers() {
    let space = Workspace::create().await;
    let (own, other) = (
        space.fleet("own", Grants::default()).await,
        space.fleet("other", Grants::default()).await,
    );
    let pushed = [delta(KEY, PINNED_CATEGORY, Visibility::Fleet)];
    for fleet in [&own, &other] {
        space
            .memories
            .capture(fleet, &pushed, AT)
            .await
            .expect("a capture");
    }

    space
        .memories
        .forget(&space.id, &own, KEY)
        .await
        .expect("the fleet's own entry is forgotten");

    let keys = |hydrated: afd_wire::memory::MemoryHydrateResponse<'static>| {
        hydrated
            .memory
            .into_iter()
            .map(|entry| entry.key.into_owned())
            .collect::<Vec<_>>()
    };
    let mine = space.memories.hydrate(&own).await.expect("a hydrate");
    let theirs = space.memories.hydrate(&other).await.expect("a hydrate");
    assert!(keys(mine).is_empty(), "the forgotten entry is gone");
    assert_eq!(
        keys(theirs),
        [KEY],
        "the other fleet's entry under the key stays"
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_forget_of_a_key_the_fleet_does_not_hold_answers_entry_not_found() {
    let space = Workspace::create().await;
    let fleet = space.fleet("empty", Grants::default()).await;

    let refused = space
        .memories
        .forget(&space.id, &fleet, KEY)
        .await
        .expect_err("a key the fleet never held is refused");

    assert_eq!(
        (refused.code(), refused.detail()),
        (MEM_ENTRY_NOT_FOUND, ENTRY_NOT_FOUND)
    );
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_every_operator_verb_refuses_a_fleet_outside_the_workspace_alike() {
    let space = Workspace::create().await;
    let fleet = space.fleet("elsewhere", Grants::default()).await;
    let (stranger, ghost) = (mint_uuid7(), mint_uuid7());
    let memories = &space.memories;
    let change = MemoryAccessRequest {
        read: Some(true),
        publish: None,
    };

    // A real fleet named under the wrong workspace, and a fleet that does not
    // exist, answer the same: no probe learns which identifiers are real.
    let refusals = [
        memories.forget(&stranger, &fleet, KEY).await.map(drop),
        memories
            .page(&stranger, &fleet, View::Recent, None, 1)
            .await
            .map(drop),
        memories
            .set_access(&stranger, &fleet, change)
            .await
            .map(drop),
        memories.forget(&space.id, &ghost, KEY).await.map(drop),
        memories.capture(&ghost, &[], AT).await.map(drop),
        memories.hydrate(&ghost).await.map(drop),
    ];

    for refused in refusals {
        let refused = refused.expect_err("a fleet the workspace does not hold is refused");
        assert_eq!(
            (refused.code(), refused.detail()),
            (MEM_AGENTSFLEET_NOT_FOUND, FLEET_NOT_FOUND)
        );
    }
    space.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_grant_change_answers_both_grants_and_holds_for_the_next_read() {
    let space = Workspace::create().await;
    let fleet = space.fleet("reader", Grants::default()).await;
    let change = MemoryAccessRequest {
        read: Some(true),
        publish: None,
    };

    let answered = space
        .memories
        .set_access(&space.id, &fleet, change)
        .await
        .expect("a grant change");

    assert_eq!(
        answered,
        MemoryAccess {
            read: true,
            publish: false
        }
    );
    let hydrated = space.memories.hydrate(&fleet).await.expect("a hydrate");
    assert!(!hydrated.publish, "publish was not named, so it stands");
    space.cleanup().await;
}
