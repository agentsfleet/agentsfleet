//! `fleet.admission_backlogs` follows the ledger: an insert counts at once, a
//! first settle releases at commit, and the runtime role cannot move the count.
//!
//! Each case reads the count with its own SQL, because what is under test is
//! what the triggers hold, not what the crate believes.

use std::time::Duration;

use afd_admission::{Admissions, Reply as ReplyTo};
use afd_db::test_util::mint_id;
use sqlx::Connection as _;

use super::{
    lane::Lane,
    ledger::{admit, steer},
    queue::over_fake,
};

/// SQLSTATE `insufficient_privilege`.
const INSUFFICIENT_PRIVILEGE: &str = "42501";

/// How long an admission may take while another transaction holds a settle open.
const INTAKE_WAIT: Duration = Duration::from_secs(2);

/// The fleet's stored count, or `None` before its first admission.
async fn stored_count(lane: &Lane) -> Option<i64> {
    sqlx::query_scalar(
        "SELECT unsettled_count FROM fleet.admission_backlogs WHERE fleet_id = $1::uuid",
    )
    .bind(&lane.fleet)
    .fetch_optional(&mut *lane.database.acquire().await.expect("observer"))
    .await
    .expect("reading the stored count")
}

/// The fleet's unsettled rows, counted from the ledger itself.
async fn unsettled_rows(lane: &Lane) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions WHERE fleet_id = $1::uuid AND settled_at IS NULL",
    )
    .bind(&lane.fleet)
    .fetch_one(&mut *lane.database.acquire().await.expect("observer"))
    .await
    .expect("counting unsettled rows")
}

/// Settles one row the way a report does.
async fn settle(lane: &Lane, id: &str) {
    sqlx::query(
        "UPDATE fleet.admissions SET settled_at = COALESCE(settled_at, clock_timestamp()) \
         WHERE id = $1::uuid",
    )
    .bind(id)
    .execute(&mut *lane.database.acquire().await.expect("settler"))
    .await
    .expect("settling a row");
}

/// Admits `n` steers under fresh keys and returns their row ids.
async fn admit_many(lane: &Lane, admissions: &Admissions, n: usize) -> Vec<String> {
    let mut ids = Vec::with_capacity(n);
    for _ in 0..n {
        let key = mint_id();
        ids.push(
            admit(admissions, steer(lane, &key, ReplyTo::None))
                .await
                .stored
                .id,
        );
    }
    ids
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn backlog_count_follows_inserts_and_settles() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let ids = admit_many(&lane, &admissions, 3).await;
    assert_eq!(stored_count(&lane).await, Some(3));
    assert_eq!(unsettled_rows(&lane).await, 3);
    settle(&lane, ids.first().expect("three rows")).await;
    assert_eq!(stored_count(&lane).await, Some(2));
    assert_eq!(unsettled_rows(&lane).await, 2);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn restamping_a_settled_row_releases_nothing() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let ids = admit_many(&lane, &admissions, 3).await;
    let first = ids.first().expect("three rows");
    settle(&lane, first).await;
    settle(&lane, first).await;
    assert_eq!(stored_count(&lane).await, Some(2));
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_rolled_back_insert_leaves_the_count() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    admit_many(&lane, &admissions, 1).await;
    let key = mint_id();
    let prepared = admissions
        .prepare(steer(&lane, &key, ReplyTo::None))
        .await
        .expect("budget observation");
    let mut connection = lane.database.acquire().await.expect("transaction");
    let mut tx = connection.begin().await.expect("begin");
    admissions
        .stage(&mut tx, prepared)
        .await
        .expect("stage locally");
    tx.rollback().await.expect("rollback");
    drop(connection);
    assert_eq!(stored_count(&lane).await, Some(1));
    assert_eq!(unsettled_rows(&lane).await, 1);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn runtime_role_cannot_set_the_backlog_count() {
    let lane = Lane::seed().await;
    let refusals = [
        "UPDATE fleet.admission_backlogs SET unsettled_count = 0 WHERE fleet_id = $1::uuid",
        "INSERT INTO fleet.admission_backlogs (fleet_id, unsettled_count, created_at, updated_at) \
         VALUES ($1::uuid, 0, now(), now())",
    ];
    for statement in refusals {
        let failure = sqlx::query(statement)
            .bind(&lane.fleet)
            .execute(&mut *lane.database.acquire().await.expect("api connection"))
            .await
            .expect_err("the runtime role may not write the count");
        let code = failure
            .as_database_error()
            .and_then(sqlx::error::DatabaseError::code);
        assert_eq!(code.as_deref(), Some(INSUFFICIENT_PRIVILEGE), "{statement}");
    }
    assert_eq!(stored_count(&lane).await, None);
    lane.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_open_settle_does_not_hold_intake() {
    let lane = Lane::seed().await;
    let (_server, admissions) = over_fake(&lane, &[]).await;
    let ids = admit_many(&lane, &admissions, 1).await;
    let mut holder = lane.database.acquire().await.expect("holder");
    let holder_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *holder)
        .await
        .expect("holder backend");
    let mut report = holder.begin().await.expect("open report");
    sqlx::query("UPDATE fleet.admissions SET settled_at = clock_timestamp() WHERE id = $1::uuid")
        .bind(ids.first().expect("one row"))
        .execute(&mut *report)
        .await
        .expect("settle inside the open report");
    let key = mint_id();
    let admitted = tokio::time::timeout(
        INTAKE_WAIT,
        admissions.admit(steer(&lane, &key, ReplyTo::None)),
    )
    .await
    .expect("intake is not held behind the open report")
    .expect("admission");
    assert!(!admitted.replayed);
    let blocked: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid))",
    )
    .bind(holder_pid)
    .fetch_one(&mut *lane.database.acquire().await.expect("autocommit observer"))
    .await
    .expect("reading blocked sessions");
    assert_eq!(blocked, 0);
    assert_eq!(stored_count(&lane).await, Some(2));
    report.commit().await.expect("report commits");
    drop(holder);
    assert_eq!(stored_count(&lane).await, Some(1));
    lane.cleanup().await;
}
