//! Held affinity reads the authoritative ledger, and an idle poll costs one statement.
#![expect(clippy::expect_used, reason = "test assertions")]

use super::{hold, id, live};
use crate::seed::{Seeded, seeded, seeded_parts};
use crate::support::Fixtures;
use afd_core::timing::SANDBOX_HOLD_IDLE;

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
async fn test_an_idle_poll_costs_one_claim_statement_and_no_dragonfly_commands() {
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, _, _, [holder]) = seeded_parts::<1>(&fixtures).await;
    let (polled, cost) = fixtures
        .leases()
        .select_measured(&holder, &[id(&fleet)])
        .await;
    assert!(polled.expect("poll").is_none());
    assert_eq!(cost.database_roundtrips, 1, "{cost:?}");
    assert_eq!(cost.dragonfly_commands, 0, "{cost:?}");
    fixtures.cleanup().await;
}
