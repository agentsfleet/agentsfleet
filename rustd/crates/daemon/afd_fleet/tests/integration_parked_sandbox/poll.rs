//! Parked affinity and notification loss both use the authoritative ledger.
#![expect(clippy::expect_used, reason = "test assertions")]

use super::{live, park};
use crate::seed::{Seeded, seeded, seeded_parts};
use crate::support::Fixtures;
use afd_runner::timing::SANDBOX_PARK_IDLE;

use afd_bus::wake::Wake;
use afd_db::test_util::mint_id;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_naming_a_parked_fleet_wins_another_runner_nothing() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [keeper, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let runs = fixtures.runs();
    live(&fixtures, &keeper).await;
    park(
        &runs,
        &fixtures,
        &fleet,
        &keeper,
        fixtures.after(SANDBOX_PARK_IDLE),
    )
    .await;
    let named = [fleet.clone()];
    assert!(runs.select(&other, &named).await.expect("poll").is_none());
    let acquired = runs
        .select(&keeper, &named)
        .await
        .expect("poll")
        .expect("keeper gets work");
    assert_eq!(acquired.fleet_id, fleet);
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_idle_poll_examines_no_candidates() {
    let fixtures = Fixtures::create().await;
    let (fleet, _, _, [keeper]) = seeded_parts::<1>(&fixtures).await;
    let (polled, cost) = fixtures
        .runs()
        .select_measured(&keeper, std::slice::from_ref(&fleet))
        .await;
    assert!(polled.expect("poll").is_none());
    assert_eq!(cost.candidates_scanned, 0, "{cost:?}");
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_stale_notification_does_not_hide_unnotified_work() {
    let fixtures = Fixtures::create().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let bell = Wake::default();
    bell.ring(mint_id());
    let runs = fixtures.runs().with_wake(bell);
    let acquired = runs
        .select(&runner, &[])
        .await
        .expect("fallback probe")
        .expect("work found");
    assert_eq!(acquired.fleet_id, fleet);
    fixtures.cleanup().await;
}
