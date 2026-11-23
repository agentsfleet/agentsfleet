//! Idle holds consume new-work capacity and never renew an existing run.

use std::sync::Arc;

use af_core::clock;
use afd_runner::capacity::Work;

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

    assert!(!fixture.capacity(Work::Fleet(&other)).await.can_assign());
    assert!(!fixture.capacity(Work::Setup).await.can_assign());
    assert!(!fixture.try_assign().await, "setup writes no third run");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_fleets_own_hold_takes_no_second_worker() {
    let fixture = Fixture::create(2).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    let held_fleet = fixture.hold(clock::after(now, LIVE_FOR)).await;

    assert!(
        fixture
            .capacity(Work::Fleet(&held_fleet))
            .await
            .can_assign()
    );
    assert!(!fixture.capacity(Work::Setup).await.can_assign());
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_renewal_counts_no_holds() {
    let fixture = Fixture::create(1).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.hold(clock::after(now, LIVE_FOR)).await;

    assert!(fixture.capacity(Work::Existing).await.can_renew());
    assert!(!fixture.capacity(Work::Setup).await.can_assign());
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_expired_hold_does_not_reserve_a_worker() {
    let fixture = Fixture::create(1).await;
    fixture.hold(clock::before(clock::now(), LAPSED_BY)).await;

    assert!(fixture.try_assign().await);
    assert!(
        !fixture.try_assign().await,
        "the first assignment filled the host"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_assignments_leave_room_for_a_live_hold() {
    let fixture = Arc::new(Fixture::create(2).await);
    fixture.hold(clock::after(clock::now(), LIVE_FOR)).await;
    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let mut assignments = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let fixture = Arc::clone(&fixture);
        let barrier = Arc::clone(&barrier);
        assignments.spawn(async move {
            barrier.wait().await;
            fixture.try_assign().await
        });
    }
    let mut accepted = 0;
    while let Some(assignment) = assignments.join_next().await {
        if assignment.expect("assignment task completes") {
            accepted += 1;
        }
    }

    assert_eq!(accepted, 1, "the idle hold retains the other worker");
    Arc::try_unwrap(fixture)
        .ok()
        .expect("assignments finished")
        .cleanup()
        .await;
}
