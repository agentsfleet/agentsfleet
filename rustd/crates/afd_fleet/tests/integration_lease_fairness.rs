//! Ineligible fleets cannot fill the bounded candidate list ahead of eligible work.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::{queue, requests, seed, support::Fixtures};
use afd_core::id::Uuid7;
use afd_db::test_util::mint_id;
use afd_dragonfly::Doorbell;
use afd_wire::runner::{NetworkPolicy, RegisterRequest, SandboxTier};
use std::borrow::Cow;
use std::collections::BTreeSet;

const HOT_FLEETS: usize = 96;
const PLACEABLE: usize = 3;
const HOT_TAG: &str = "fixture:nobody-carries-this";

async fn enqueue(fixtures: &Fixtures, fleet: &str, workspace: &str) {
    queue::enqueue(
        &fixtures.database,
        fleet,
        workspace,
        seed::ACTOR,
        seed::EVENT_TYPE,
        seed::REQUEST_JSON,
        fixtures.enrolled_at(),
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
        assert!(
            cost.candidates_scanned <= u64::try_from(remaining).expect("small count"),
            "{cost:?}"
        );
        found.insert(acquired.fleet_id.as_str().to_owned());
    }
    assert_eq!(found, expected);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_one_notification_prioritizes_its_fleet_without_hiding_the_rest() {
    let fixtures = Fixtures::create_with_queue().await;
    let (first, workspace, tenant) = seed::unique_ids();
    let second = mint_id();
    let mut request = enrolment(&first);
    for fleet in [&first, &second] {
        let tag = requests::placement_tag(fleet);
        fixtures.seed_fleet(fleet, &workspace, &tenant, &tag).await;
        request.labels.push(Cow::Owned(tag));
        enqueue(&fixtures, fleet, &workspace).await;
    }
    let runner = fixtures
        .runners()
        .register(&request, fixtures.enrolled_at())
        .await
        .expect("runner")
        .runner_id;
    let bell = Doorbell::new();
    bell.ring(Uuid7::parse(&second).expect("fleet"));
    let leases = fixtures.leases().with_doorbell(bell);
    assert_eq!(
        leases
            .select(&runner, &[])
            .await
            .expect("poll")
            .expect("rung fleet")
            .fleet_id
            .as_str(),
        second
    );
    assert_eq!(
        leases
            .select(&runner, &[])
            .await
            .expect("poll")
            .expect("unnotified fleet")
            .fleet_id
            .as_str(),
        first
    );
    fixtures.cleanup().await;
}
