//! Terminal history settles a reclaimed admission without executing another run.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::report_seed::held;
use crate::rows::{LAPSE, Row};
use afd_core::id::Uuid7;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_completed_event_redelivery_does_not_execute() {
    let run = held().await;
    run.fixtures
        .end_event(
            &run.fleet,
            &run.event_id,
            afd_core::event::status::PROCESSED,
        )
        .await;
    run.fixtures
        .age(
            Row::Lease(run.issued.lease_id.as_str()),
            "lease_expires_at",
            LAPSE,
        )
        .await;
    run.fixtures
        .age(Row::Slot(&run.fleet), "lease_expires_at", LAPSE)
        .await;
    let fleet = Uuid7::parse(&run.fleet).expect("fleet");
    let answer = run
        .fixtures
        .plane()
        .lease(&run.spare, &[fleet], false, run.now)
        .await
        .expect("poll");
    assert!(answer.contains("\"lease\":null"), "{answer}");
    let mut connection = run.fixtures.database.acquire().await.expect("database");
    let settled: bool = sqlx::query_scalar(
        "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1::uuid",
    )
    .bind(&run.event_id)
    .fetch_one(&mut *connection)
    .await
    .expect("queue state");
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE fleet_id = $1::uuid")
            .bind(&run.fleet)
            .fetch_one(&mut *connection)
            .await
            .expect("lease count");
    assert!(settled, "the terminal admission leaves the queue");
    assert_eq!(attempts, 1, "no second execution");
    drop(connection);
    assert!(
        run.leases
            .select(&run.spare, &[])
            .await
            .expect("next poll")
            .is_none()
    );
    run.fixtures.cleanup().await;
}
