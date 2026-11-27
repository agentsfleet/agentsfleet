//! Idle parks take no worker from new work and never renew an existing run.

use std::sync::Arc;

use af_core::clock;
use afd_runner::capacity::Work;

use super::{Fixture, fixture::LIVE_FOR};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_run_and_a_park_leave_a_third_fleet_its_worker() {
    let fixture = Fixture::create(2).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.park(clock::after(now, LIVE_FOR)).await;
    let other = afd_db::test_util::mint_id();

    assert!(fixture.capacity(Work::Fleet(&other)).await.can_assign());
    assert!(
        fixture.try_assign().await,
        "a third fleet takes the free worker"
    );
    assert!(!fixture.try_assign().await, "two live runs fill the host");
    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_renewal_counts_no_parks() {
    let fixture = Fixture::create(1).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.park(clock::after(now, LIVE_FOR)).await;

    let other = afd_db::test_util::mint_id();
    assert!(fixture.capacity(Work::Existing).await.can_renew());
    assert!(!fixture.capacity(Work::Fleet(&other)).await.can_assign());
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_assignments_fill_every_worker_beside_a_live_park() {
    let fixture = Arc::new(Fixture::create(2).await);
    fixture.park(clock::after(clock::now(), LIVE_FOR)).await;
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

    assert_eq!(accepted, 2, "the idle park takes neither worker");
    Arc::try_unwrap(fixture)
        .ok()
        .expect("assignments finished")
        .cleanup()
        .await;
}
