//! A native wait cannot spend or access an external service after authority ends.
use super::{Waiting, counts, exercise, wait_state};
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::TaskState;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn exhausted_wait_stops_before_credential_mint_or_dependency_read() {
    for (statement, reason) in [
        (
            "UPDATE fleet.tasks SET used_model_nanos=max_model_nanos WHERE id=$1::uuid",
            "cumulative_limit_reached",
        ),
        (
            "UPDATE fleet.tasks SET used_running_nanos=max_running_nanos WHERE id=$1::uuid",
            "cumulative_limit_reached",
        ),
        (
            "UPDATE fleet.tasks SET used_leases=max_leases WHERE id=$1::uuid",
            "lease_limit_reached",
        ),
    ] {
        exercise(async |case| {
            let waiting = Box::pin(Waiting::create(case)).await;
            // Existing measured usage has consumed the immutable cumulative allowance.
            let changed = sqlx::query(statement)
                .bind(&waiting.task.task_id)
                .execute(
                    &mut *case
                        .held
                        .fixtures
                        .database
                        .acquire()
                        .await
                        .expect("spent allowance fixture"),
                )
                .await
                .expect("valid preexisting cumulative usage");
            assert_eq!(changed.rows_affected(), 1);
            waiting.due().await;
            stopped(&waiting, TaskState::Exhausted, reason).await;
            waiting.finish().await;
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn revoked_github_grant_stops_wait_before_credential_mint_or_dependency_read() {
    exercise(async |case| {
        let waiting = Box::pin(Waiting::create(case)).await;
        let changed = sqlx::query("UPDATE connector.integration_grants SET status='revoked',revoked_at=clock_timestamp() WHERE fleet_id=$1::uuid AND service='github'")
            .bind(&case.held.fleet).execute(&mut *case.held.fixtures.database.acquire().await.expect("grant revocation fixture"))
            .await.expect("standing authority revoked");
        assert_eq!(changed.rows_affected(),1);
        waiting.due().await;
        stopped(&waiting,TaskState::Blocked,"dependency_access_unavailable").await;
        waiting.finish().await;
    }).await;
}

async fn stopped(waiting: &Waiting<'_>, state: TaskState, reason: &str) {
    let result = waiting
        .tasks
        .maintain_waits()
        .await
        .expect("refusal before external work");
    assert_eq!((result.checked, result.woken, result.deferred), (0, 0, 0));
    let case = waiting.fixture.case;
    let stored: (String, String, i64, i64) = sqlx::query_as(
        "SELECT status,status_reason,generation,stop_generation FROM fleet.tasks WHERE id=$1::uuid",
    )
    .bind(&waiting.task.task_id)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("stopped task observer"),
    )
    .await
    .expect("native refusal state");
    assert_eq!(stored, (state.as_str().to_owned(), reason.to_owned(), 3, 2));
    assert_eq!(wait_state(waiting).await, (false, true, false, false, 0));
    assert_eq!(counts(case, &waiting.task.task_id).await, (0, 0, 0, 2, 1));
    assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
}
