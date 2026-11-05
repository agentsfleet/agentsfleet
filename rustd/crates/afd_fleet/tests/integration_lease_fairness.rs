//! Ineligible fleets cannot hide eligible work, and ready fleets are claimed
//! oldest first.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::{queue, requests, seed, support::Fixtures};
use afd_db::test_util::mint_id;
use afd_wire::runner::{NetworkPolicy, RegisterRequest, SandboxTier};
use std::borrow::Cow;
use std::collections::BTreeSet;

const HOT_FLEETS: usize = 96;
const PLACEABLE: usize = 3;
const HOT_TAG: &str = "fixture:nobody-carries-this";

async fn enqueue(fixtures: &Fixtures, fleet: &str, workspace: &str) {
    enqueue_at(fixtures, fleet, workspace, fixtures.enrolled_at()).await;
}

async fn enqueue_at(fixtures: &Fixtures, fleet: &str, workspace: &str, at: jiff::Timestamp) {
    queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        seed::ACTOR,
        seed::EVENT_TYPE,
        seed::REQUEST_JSON,
        at,
    )
    .await;
}

fn enrolment(fleet: &str) -> RegisterRequest<'static> {
    requests::enrolment_tagged(
        SandboxTier::LandlockFull,
        NetworkPolicy::AllowListEgress,
        1,
        &requests::placement_tag(fleet),
    )
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_ineligible_work_cannot_hide_eligible_fleets() {
    let fixtures = Fixtures::create_with_queue().await;
    let (base, workspace, tenant) = seed::unique_ids();
    for _ in 0..HOT_FLEETS {
        let fleet = mint_id();
        fixtures
            .seed_fleet(&fleet, &workspace, &tenant, HOT_TAG)
            .await;
        enqueue(&fixtures, &fleet, &workspace).await;
    }
    let mut expected = BTreeSet::new();
    let mut request = enrolment(&base);
    for _ in 0..PLACEABLE {
        let fleet = mint_id();
        let tag = requests::placement_tag(&fleet);
        fixtures.seed_fleet(&fleet, &workspace, &tenant, &tag).await;
        request.labels.push(Cow::Owned(tag));
        enqueue(&fixtures, &fleet, &workspace).await;
        expected.insert(fleet);
    }
    let runner = fixtures
        .runners()
        .register(&request, fixtures.enrolled_at())
        .await
        .expect("runner")
        .runner_id;
    let leases = fixtures.leases();
    let mut found = BTreeSet::new();
    for remaining in (1..=PLACEABLE).rev() {
        let (answer, cost) = leases.select_measured(&runner, &[]).await;
        let acquired = answer.expect("poll").expect("eligible work");
        assert_eq!(
            cost.database_roundtrips, 1,
            "one claim statement with {remaining} placeable left: {cost:?}"
        );
        found.insert(acquired.fleet_id.as_str().to_owned());
    }
    assert_eq!(found, expected);
    fixtures.cleanup().await;
}

/// Dimension 2.6: three ready fleets armed at ascending instants, written in
/// another order, with no hold and no affinity: the claim takes the oldest.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_claim_takes_the_oldest_ready_fleet() {
    let fixtures = Fixtures::create_with_queue().await;
    let (first, workspace, tenant) = seed::unique_ids();
    let fleets = [first, mint_id(), mint_id()];
    let mut request = enrolment(&fleets[0]);
    for fleet in &fleets {
        let tag = requests::placement_tag(fleet);
        fixtures.seed_fleet(fleet, &workspace, &tenant, &tag).await;
        request.labels.push(Cow::Owned(tag));
    }
    // Written newest first, so the order claimed is the instants' and not the
    // order the rows landed in.
    for (offset, fleet) in [(2, &fleets[2]), (0, &fleets[0]), (1, &fleets[1])] {
        let at = afd_core::clock::after(
            fixtures.enrolled_at(),
            std::time::Duration::from_secs(offset),
        );
        enqueue_at(&fixtures, fleet, &workspace, at).await;
    }
    let runner = fixtures
        .runners()
        .register(&request, fixtures.enrolled_at())
        .await
        .expect("runner")
        .runner_id;
    let leases = fixtures.leases();
    let mut claimed = Vec::new();
    for _ in &fleets {
        let acquired = leases
            .select(&runner, &[])
            .await
            .expect("poll")
            .expect("ready work");
        claimed.push(acquired.fleet_id.as_str().to_owned());
    }
    assert_eq!(claimed, fleets.to_vec(), "oldest ready first");
    fixtures.cleanup().await;
}
