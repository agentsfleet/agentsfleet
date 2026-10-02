#![expect(
    clippy::unwrap_used,
    clippy::assertions_on_result_states,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::time::Duration;

use afd_core::id::Uuid7;

use super::FleetTurns;
use crate::test_support::{FLEET_ID, LEASE_ID};

#[tokio::test(start_paused = true)]
async fn one_fleet_runs_one_at_a_time_and_others_do_not_wait() {
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();
    let other = Uuid7::parse(LEASE_ID).unwrap();

    let held = turns.claim(&fleet).await.unwrap();
    let waiting = tokio::time::timeout(Duration::from_secs(1), turns.claim(&fleet)).await;
    let elsewhere = turns.claim(&other).await;
    let queued = tokio::spawn({
        let turns = turns.clone();
        let fleet = fleet.clone();
        async move { turns.claim(&fleet).await.is_some() }
    });
    tokio::task::yield_now().await;
    drop(held);

    assert!(waiting.is_err(), "a busy fleet makes the next claim wait");
    assert!(elsewhere.is_some(), "a different fleet is not held up");
    assert!(
        queued.await.unwrap(),
        "the waiter gets the fleet once it is freed"
    );
}

#[tokio::test(start_paused = true)]
async fn a_claimer_that_gave_up_does_not_keep_the_fleet() {
    let (turns, coordinator) = FleetTurns::start();
    tokio::spawn(coordinator);
    let fleet = Uuid7::parse(FLEET_ID).unwrap();
    let held = turns.claim(&fleet).await.unwrap();
    // This claimer gives up while it waits; its grant is never read.
    let abandoned = tokio::time::timeout(Duration::from_millis(10), turns.claim(&fleet)).await;
    drop(held);

    let next = tokio::time::timeout(Duration::from_secs(1), turns.claim(&fleet)).await;

    assert!(abandoned.is_err());
    assert!(
        next.unwrap().is_some(),
        "the abandoned grant freed the fleet"
    );
}

#[tokio::test]
async fn a_claim_after_the_coordinator_stopped_gets_nothing() {
    let (turns, coordinator) = FleetTurns::start();
    drop(coordinator);

    assert!(
        turns
            .claim(&Uuid7::parse(FLEET_ID).unwrap())
            .await
            .is_none()
    );
}
