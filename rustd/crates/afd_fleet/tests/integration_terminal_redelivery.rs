//! Terminal history settles a reclaimed admission without executing another run.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::report_seed::{Held, held, held_in};
use crate::rows::{LAPSE, Row};
use crate::support::Fixtures;
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

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn terminal_redelivery_cannot_settle_after_its_claim_is_superseded() {
    let run = held_in(Fixtures::create_isolated_with_queue().await).await;
    run.fixtures
        .end_event(
            &run.fleet,
            &run.event_id,
            afd_core::event::status::PROCESSED,
        )
        .await;
    let owner = run.fixtures.open(afd_db::config::DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    sqlx::raw_sql(
        "CREATE FUNCTION fleet.supersede_delivered_claim() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
           UPDATE runner.fleet_slots SET fencing_token=fencing_token+1 WHERE fleet_id=NEW.fleet_id; \
           NEW.lease_fence=NEW.lease_fence+1; RETURN NEW; END $$; \
         CREATE TRIGGER supersede_delivered_claim BEFORE UPDATE OF delivered_at ON fleet.admissions \
         FOR EACH ROW EXECUTE FUNCTION fleet.supersede_delivered_claim()",
    ).execute(&mut *connection).await.expect("supersede between receive and settlement");
    let answer = run
        .fixtures
        .plane()
        .lease_claimed(run.acquired.clone(), &run.runner, run.now)
        .await
        .expect("stale terminal completion is harmless");
    assert!(answer.contains("\"lease\":null"));
    assert_superseded_claim_retained(&run).await;
    sqlx::raw_sql("DROP TRIGGER supersede_delivered_claim ON fleet.admissions; DROP FUNCTION fleet.supersede_delivered_claim()")
        .execute(&mut *connection).await.expect("remove fault");
    drop(connection);
    owner.close().await;
    run.fixtures
        .age(
            Row::Lease(run.lease_key.as_str()),
            "lease_expires_at",
            LAPSE,
        )
        .await;
    run.fixtures
        .age(Row::Slot(&run.fleet), "lease_expires_at", LAPSE)
        .await;
    let answer = run
        .fixtures
        .plane()
        .lease(
            &run.spare,
            &[Uuid7::parse(&run.fleet).expect("fleet")],
            false,
            run.now,
        )
        .await
        .expect("current claimant suppresses the terminal event");
    assert!(answer.contains("\"lease\":null"));
    assert!(run.fixtures.admission_settled(&run.event_id).await);
    run.fixtures.cleanup().await;
}

async fn assert_superseded_claim_retained(run: &Held) {
    let state: (bool, Option<i64>, i64, i64) = sqlx::query_as(
        "SELECT a.settled_at IS NOT NULL,a.lease_fence,s.fencing_token, \
         (SELECT count(*) FROM runner.leases WHERE fleet_id=a.fleet_id) \
         FROM fleet.admissions a JOIN runner.fleet_slots s ON s.fleet_id=a.fleet_id \
         WHERE a.id=$1::uuid AND s.last_runner_id=$2::uuid",
    )
    .bind(&run.event_id)
    .bind(run.runner.as_str())
    .fetch_one(&mut *run.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("superseded claim still exists");
    let fence = run.fence.as_i64() + 1;
    assert_eq!(state, (false, Some(fence), fence, 1));
}
