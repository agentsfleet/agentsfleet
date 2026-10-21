//! The fleet row lock makes backlog refusal exact under concurrent admission.

use super::{
    lane::Lane,
    ledger::{admit, steer},
    queue::over_fake,
};
use afd_admission::{Budgets, Reply as ReplyTo};
use afd_db::test_util::mint_id;
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_hundred_concurrent_producers_cannot_overrun_one_fleet_budget() {
    let lane = Arc::new(Lane::seed().await);
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let admissions = admissions.with_budgets(Budgets {
        fleet_backlog: 5,
        deployment_backlog: afd_admission::budget::DEPLOYMENT_BACKLOG_BUDGET,
    });
    let barrier = Arc::new(tokio::sync::Barrier::new(100));
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..100 {
        let (lane, admissions, barrier) =
            (Arc::clone(&lane), admissions.clone(), Arc::clone(&barrier));
        tasks.spawn(async move {
            let key = mint_id();
            barrier.wait().await;
            admissions.admit(steer(&lane, &key, ReplyTo::None)).await
        });
    }
    let mut accepted = 0;
    let mut refused = 0;
    while let Some(result) = tasks.join_next().await {
        match result.expect("producer task") {
            Ok(_) => accepted += 1,
            Err(error) => {
                assert!(error.is_over_capacity(), "{error}");
                refused += 1;
            }
        }
    }
    assert_eq!((accepted, refused), (5, 95));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE fleet_id=$1::uuid")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.database.acquire().await.expect("database"))
            .await
            .expect("committed row count");
    assert_eq!(count, 5);
    Arc::into_inner(lane)
        .expect("producer tasks released the fixture")
        .cleanup()
        .await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn delivered_unsettled_rows_consume_budget_and_retries_still_succeed() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let admissions = admissions.with_budgets(Budgets {
        fleet_backlog: 1,
        deployment_backlog: afd_admission::budget::DEPLOYMENT_BACKLOG_BUDGET,
    });
    let first = admit(&admissions, steer(&lane, &lane.fleet, ReplyTo::None)).await;
    sqlx::query("UPDATE fleet.admissions SET delivered_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&first.stored.id)
        .execute(&mut *lane.database.acquire().await.expect("database"))
        .await
        .expect("delivered row remains unsettled");
    assert!(
        admissions
            .admit(steer(&lane, &mint_id(), ReplyTo::None))
            .await
            .expect_err("full budget")
            .is_over_capacity()
    );
    let retry = admit(&admissions, steer(&lane, &lane.fleet, ReplyTo::None)).await;
    assert!(retry.replayed);
    assert_eq!(retry.stored.id, first.stored.id);
    sqlx::query("UPDATE fleet.admissions SET settled_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&first.stored.id)
        .execute(&mut *lane.database.acquire().await.expect("database"))
        .await
        .expect("settlement releases capacity");
    let next = admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None)).await;
    assert!(!next.replayed);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_fleet_budget_counts_unsettled_rows() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let first = admit(&admissions, steer(&lane, &lane.fleet, ReplyTo::None)).await;
    let ids: Vec<_> = (1..10_000).map(|_| mint_id()).collect();
    sqlx::query("INSERT INTO fleet.admissions (id, fleet_id, workspace_id, producer, producer_key, payload_digest, actor, event_type, request_json, event_created_at, created_at, updated_at) SELECT seed.id::uuid, a.fleet_id, a.workspace_id, a.producer, seed.id, a.payload_digest, a.actor, a.event_type, a.request_json, now(), now(), now() FROM fleet.admissions a CROSS JOIN unnest($2::text[]) AS seed(id) WHERE a.id=$1::uuid")
        .bind(&first.stored.id).bind(&ids)
        .execute(&mut *lane.database.acquire().await.expect("database"))
        .await.expect("populate full budget");
    assert!(
        admissions
            .admit(steer(&lane, &mint_id(), ReplyTo::None))
            .await
            .expect_err("10,000 outstanding")
            .is_over_capacity()
    );
    sqlx::query("UPDATE fleet.admissions SET settled_at=clock_timestamp() WHERE id=$1::uuid")
        .bind(&first.stored.id)
        .execute(&mut *lane.database.acquire().await.expect("database"))
        .await
        .expect("settle one");
    assert!(
        !admit(&admissions, steer(&lane, &mint_id(), ReplyTo::None))
            .await
            .replayed
    );
    lane.cleanup().await;
}
