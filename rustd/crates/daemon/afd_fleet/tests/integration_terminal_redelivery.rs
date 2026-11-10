//! Terminal history settles a reclaimed admission without executing another run.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::report_seed::{Held, held, held_in};
use crate::rows::{LAPSE, Row};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_completed_event_redelivery_does_not_execute() {
    let run = held().await;
    run.fixtures
        .end_event(&run.fleet, &run.event_id, af_core::event::status::PROCESSED)
        .await;
    age_original_claim(&run).await;
    let fleet = run.fleet.clone();
    let answer = run
        .fixtures
        .plane()
        .claim_fleet_with(
            afd_fleet::lease::OfferRequest {
                runner: &run.spare,
                held: &[fleet],
                degraded: false,
                now: run.now,
            },
            None,
            |_lease, _metadata| (),
        )
        .await
        .expect("poll");
    assert!(answer.is_none(), "{answer:?}");
    let mut connection = run.fixtures.database.acquire().await.expect("database");
    let settled: bool =
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id = $1")
            .bind(&run.event_id)
            .fetch_one(&mut *connection)
            .await
            .expect("queue state");
    let attempts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM runner.leases WHERE fleet_id = $1")
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
    let run = held_in(Fixtures::create_isolated().await).await;
    run.fixtures
        .end_event(&run.fleet, &run.event_id, af_core::event::status::PROCESSED)
        .await;
    // The holder stops renewing, and the finished event is claimed again: the
    // claim answers a terminal redelivery.
    run.fixtures
        .age(Row::Lease(&run.lease_key), "lease_expires_at", LAPSE)
        .await;
    run.fixtures
        .age(Row::Slot(&run.fleet), "lease_expires_at", LAPSE)
        .await;
    let again = crate::seed::select_fleet(&run.fixtures.leases(), &run.runner, &run.fleet)
        .await
        .expect("the finished admission is claimed again");
    assert_eq!(
        again.opened.received.delivery,
        afd_fleet::lease::Delivery::Terminal
    );
    // A rival supersedes that claim between the claim and its settlement.
    sqlx::query(
        "WITH slot AS (UPDATE runner.fleet_slots SET fencing_token = fencing_token + 1 \
         WHERE fleet_id = $1 RETURNING fencing_token) \
         UPDATE fleet.admissions SET lease_fence = (SELECT fencing_token FROM slot) WHERE id = $2",
    )
    .bind(&run.fleet)
    .bind(&run.event_id)
    .execute(&mut *run.fixtures.database.acquire().await.expect("rival"))
    .await
    .expect("supersede between the claim and settlement");
    let answer = run
        .fixtures
        .plane()
        .claim_selected(again.clone(), &run.runner, run.now)
        .await
        .expect("stale terminal completion is harmless");
    assert!(
        answer.is_none(),
        "the terminal event is never reissued: {answer:?}"
    );
    assert_superseded_claim_retained(&run, again.fence.as_i64() + 1).await;
    run.fixtures
        .age(Row::Slot(&run.fleet), "lease_expires_at", LAPSE)
        .await;
    let answer = reclaim_terminal(&run).await;
    assert!(
        answer.is_none(),
        "the terminal event is never reissued: {answer:?}"
    );
    assert!(run.fixtures.admission_settled(&run.event_id).await);
    run.fixtures.cleanup().await;
}

async fn assert_superseded_claim_retained(run: &Held, fence: i64) {
    let state: (bool, Option<i64>, i64, i64) = sqlx::query_as(
        "SELECT a.settled_at IS NOT NULL,a.lease_fence,s.fencing_token, \
         (SELECT count(*) FROM runner.leases WHERE fleet_id=a.fleet_id) \
         FROM fleet.admissions a JOIN runner.fleet_slots s ON s.fleet_id=a.fleet_id \
         WHERE a.id=$1 AND s.last_runner_id=$2",
    )
    .bind(&run.event_id)
    .bind(&run.runner)
    .fetch_one(&mut *run.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("superseded claim still exists");
    assert_eq!(state, (false, Some(fence), fence, 1));
}

async fn age_original_claim(run: &Held) {
    run.fixtures
        .age(Row::Lease(&run.issued.lease_id), "lease_expires_at", LAPSE)
        .await;
    run.fixtures
        .age(Row::Slot(&run.fleet), "lease_expires_at", LAPSE)
        .await;
}

async fn reclaim_terminal(run: &Held) -> Option<()> {
    run.fixtures
        .plane()
        .claim_fleet_with(
            afd_fleet::lease::OfferRequest {
                runner: &run.spare,
                held: std::slice::from_ref(&run.fleet),
                degraded: false,
                now: run.now,
            },
            None,
            |_lease, _metadata| (),
        )
        .await
        .expect("current claimant suppresses the terminal event")
}
