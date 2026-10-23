//! Held affinity and notification loss both use the authoritative ledger.
#![expect(clippy::expect_used, reason = "test assertions")]

use super::{hold, id, live};
use crate::seed::{Seeded, seeded, seeded_parts};
use crate::support::Fixtures;
use afd_core::timing::SANDBOX_HOLD_IDLE;
use afd_db::test_util::mint_id;
use afd_dragonfly::Doorbell;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_naming_a_held_fleet_wins_another_runner_nothing() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [holder, other],
        fleet,
        ..
    } = seeded::<2>(&fixtures).await;
    let leases = fixtures.leases();
    live(&fixtures, &holder).await;
    hold(
        &leases,
        &fixtures,
        &fleet,
        &holder,
        fixtures.after(SANDBOX_HOLD_IDLE),
    )
    .await;
    let named = [id(&fleet)];
    assert!(leases.select(&other, &named).await.expect("poll").is_none());
    let acquired = leases
        .select(&holder, &named)
        .await
        .expect("poll")
        .expect("holder gets work");
    assert_eq!(acquired.fleet_id, id(&fleet));
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_idle_poll_costs_one_postgres_probe_and_no_dragonfly_commands() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _, _, [holder]) = seeded_parts::<1>(&fixtures).await;
    let (polled, cost) = fixtures
        .leases()
        .select_measured(&holder, &[id(&fleet)])
        .await;
    assert!(polled.expect("poll").is_none());
    assert_eq!(cost.database_roundtrips, 1, "{cost:?}");
    assert_eq!(cost.dragonfly_commands, 0, "{cost:?}");
    assert_eq!(cost.candidates_scanned, 0, "{cost:?}");
    fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_stale_notification_does_not_hide_unnotified_work() {
    let fixtures = Fixtures::create_with_queue().await;
    let Seeded {
        runners: [runner],
        fleet,
        ..
    } = seeded::<1>(&fixtures).await;
    let bell = Doorbell::new();
    bell.ring(id(&mint_id()));
    let leases = fixtures.leases().with_doorbell(bell);
    let acquired = leases
        .select(&runner, &[])
        .await
        .expect("fallback probe")
        .expect("work found");
    assert_eq!(acquired.fleet_id, id(&fleet));
    fixtures.cleanup().await;
}
