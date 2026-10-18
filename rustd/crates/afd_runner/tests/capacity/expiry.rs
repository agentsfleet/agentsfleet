//! Both lease kinds occupy capacity only while their original expiry is live.

use super::Fixture;
use std::time::Duration;

use super::fixture::{LAPSED_BY, LIVE_FOR};
use afd_core::clock;
use sqlx::Connection as _;

/// How long the competing claimant holds the host lock.
const LOCK_HELD_FOR: Duration = Duration::from_millis(600);

/// How long the seeded lease has left when the wait starts: well inside the
/// hold, so it lapses while the claim is queued behind the lock.
const LAPSES_IN: Duration = Duration::from_millis(200);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runtime_and_authoring_leases_count_together() {
    let fixture = Fixture::create(2).await;
    fixture.runtime(clock::after(clock::now(), LIVE_FOR)).await;
    assert!(
        fixture.try_claim().await,
        "one remaining slot accepts authoring"
    );
    assert!(
        !fixture.try_claim().await,
        "runtime plus authoring exhaust the host"
    );
    fixture.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_expired_status_active_rows_do_not_occupy_capacity() {
    let fixture = Fixture::create(1).await;
    fixture
        .runtime(clock::before(clock::now(), LAPSED_BY))
        .await;
    let mut connection = fixture
        .db
        .acquire()
        .await
        .expect("authoring seed connection");
    let mut tx = connection
        .begin()
        .await
        .expect("expired attempt transaction");
    fixture
        .authoring(&mut tx, clock::before(clock::now(), LAPSED_BY))
        .await;
    tx.commit()
        .await
        .expect("expired status-active attempt seeds");
    drop(connection);
    assert!(
        fixture.try_claim().await,
        "expiry alone frees the slot before sweep"
    );
    fixture.cleanup().await;
}

/// Dimension 4.8 — a lease that lapsed while the claimant waited on the host
/// lock no longer occupies the host: the census reads the clock once the lock
/// is held, never at the instant the claim asked for it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_expiry_checks_read_clock_after_lock() {
    let fixture = Fixture::create(1).await;
    fixture.runtime(clock::after(clock::now(), LAPSES_IN)).await;
    assert!(
        !fixture.try_claim().await,
        "the live lease fills the only worker"
    );

    let mut holder = fixture.db.acquire().await.expect("competing connection");
    let mut held = holder.begin().await.expect("competing transaction");
    fixture.lock_host(&mut held).await;
    let release = async move {
        tokio::time::sleep(LOCK_HELD_FOR).await;
        held.rollback()
            .await
            .expect("the competing claimant lets go");
    };
    let (claimed, ()) = tokio::join!(fixture.try_claim(), release);

    assert!(
        claimed,
        "the lease lapsed during the wait, so the census must not count it"
    );
    drop(holder);
    fixture.cleanup().await;
}
