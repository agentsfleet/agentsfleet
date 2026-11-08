//! A policy writer holding the host row wins before the final source issue proof.
use super::super::super::fixture::{Case, exercise};
use super::{
    super::fixture::Decisions,
    support::{expire, issue, selected},
};
use sqlx::Connection as _;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn disabling_during_issue_waits_on_the_same_host_lock_and_rolls_back_the_lease() {
    exercise(race).await;
}

async fn race(case: &Case) {
    let fixture = Decisions::create(case).await;
    expire(case).await;
    let acquired = selected(case).await.expect("enabled source claim");
    case.held
        .leases
        .record_received(&acquired)
        .await
        .expect("history before host lock");
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("policy writer");
    let mut writer = connection.begin().await.expect("atomic saved policy write");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *writer)
        .await
        .expect("writer pid");
    sqlx::query("UPDATE runner.runners SET durable_tasks_enabled=false WHERE id=$1")
        .bind(&case.held.runner)
        .execute(&mut *writer)
        .await
        .expect("pending OFF edit");
    let (issued, ()) = tokio::join!(issue(case, &acquired), async {
        tokio::time::timeout(Duration::from_secs(3), blocked(case, pid))
            .await
            .expect("issue actually waits on the policy writer");
        writer.commit().await.expect("publish saved OFF policy");
    });
    assert!(
        issued.is_none(),
        "post-lock eligibility sees the committed OFF edit"
    );
    fixture.model.finish().await.expect("model cleanup");
}

async fn blocked(case: &Case, writer: i32) {
    let mut observer = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("blocking observer");
    loop {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM pg_stat_activity WHERE $1=ANY(pg_blocking_pids(pid)))",
        )
        .bind(writer)
        .fetch_one(&mut *observer)
        .await
        .expect("native blocking graph");
        if waiting {
            return;
        }
        tokio::task::yield_now().await;
    }
}
