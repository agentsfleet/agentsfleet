//! Idle holds take no worker from new work and never renew an existing run.

use af_core::clock;
use afd_runner::capacity::Work;

use super::{Fixture, fixture::LIVE_FOR};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_run_and_a_hold_leave_a_third_fleet_its_worker() {
    let fixture = Fixture::create(2).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.hold(clock::after(now, LIVE_FOR)).await;
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
async fn test_a_renewal_counts_no_holds() {
    let fixture = Fixture::create(1).await;
    let now = clock::now();
    fixture.runtime(clock::after(now, LIVE_FOR)).await;
    fixture.hold(clock::after(now, LIVE_FOR)).await;

    let other = afd_db::test_util::mint_id();
    assert!(fixture.capacity(Work::Existing).await.can_renew());
    assert!(!fixture.capacity(Work::Fleet(&other)).await.can_assign());
    fixture.cleanup().await;
}
