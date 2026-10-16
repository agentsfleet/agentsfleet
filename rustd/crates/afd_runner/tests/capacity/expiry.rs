//! Both lease kinds occupy capacity only while their original expiry is live.

use super::Fixture;
use afd_core::clock;
use sqlx::Connection as _;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_runtime_and_authoring_leases_count_together() {
    let fixture = Fixture::create(2).await;
    fixture.runtime(clock::now().as_millis() + 60_000).await;
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
    fixture.runtime(clock::now().as_millis() - 1).await;
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
        .authoring(&mut tx, clock::now().as_millis() - 1)
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
