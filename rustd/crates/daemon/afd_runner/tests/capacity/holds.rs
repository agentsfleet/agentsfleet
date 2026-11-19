//! Idle holds consume new-work capacity and never renew an existing run.

use std::sync::Arc;

use af_core::clock;
use afd_runner::capacity::Claimant;

use super::{
    Fixture,
    fixture::{LAPSED_BY, LIVE_FOR},
};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_run_and_a_hold_refuse_a_third_fleet() {
    let fixture = Fixture::create(2).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.hold(clock::after(now, LIVE_FOR)).await;
    let other = afd_db::test_util::mint_id();

    assert!(!fixture.capacity(Claimant::Fleet(&other)).await.can_claim());
    assert!(!fixture.capacity(Claimant::Authoring).await.can_claim());
    assert!(!fixture.try_claim().await, "authoring writes no third run");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_claimants_own_hold_takes_no_second_worker() {
    let fixture = Fixture::create(2).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    let held_fleet = fixture.hold(clock::after(now, LIVE_FOR)).await;

    assert!(
        fixture
            .capacity(Claimant::Fleet(&held_fleet))
            .await
            .can_claim()
    );
    assert!(!fixture.capacity(Claimant::Authoring).await.can_claim());
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_renewal_counts_no_holds() {
    let fixture = Fixture::create(1).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.hold(clock::after(now, LIVE_FOR)).await;

    assert!(fixture.capacity(Claimant::Existing).await.can_renew());
    assert!(!fixture.capacity(Claimant::Authoring).await.can_claim());
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_expired_hold_does_not_reserve_a_worker() {
    let fixture = Fixture::create(1).await;
    fixture.hold(clock::before(clock::now(), LAPSED_BY)).await;

    assert!(fixture.try_claim().await);
    assert!(
        !fixture.try_claim().await,
        "the first claim filled the host"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_claims_leave_room_for_a_live_hold() {
    let fixture = Arc::new(Fixture::create(2).await);
    fixture.hold(clock::after(clock::now(), LIVE_FOR)).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let mut claims = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let fixture = Arc::clone(&fixture);
        let barrier = Arc::clone(&barrier);
        claims.spawn(async move {
            barrier.wait().await;
            fixture.try_claim().await
        });
    }
    let mut accepted = 0;
    while let Some(claim) = claims.join_next().await {
        if claim.expect("claim task completes") {
            accepted += 1;
        }
    }

    assert_eq!(accepted, 1, "the idle hold retains the other worker");
    Arc::try_unwrap(fixture)
        .ok()
        .expect("claimants finished")
        .cleanup()
        .await;
}
