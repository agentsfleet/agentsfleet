//! Schema-valid but unreadable stored values fail closed and remain repairable.

use std::error::Error as _;

use afd_core::{error_code, id::Uuid7};
use afd_fleet::task::TaskLifecycle as _;

use super::fixture::{Case, exercise};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn corrupt_saved_state_cannot_become_success_or_disappear_from_its_owner() {
    exercise(unreadable).await;
}

async fn unreadable(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let accepted = tasks
        .bind(case.lease(), &case.plan("retained-work", 1))
        .await
        .expect("accepted work");
    let task = &accepted.items.first().expect("retained identity").task_id;
    let before = tasks
        .read(scope, task)
        .await
        .expect("complete accepted snapshot");
    let questions =
        serde_json::to_string(&before.context.questions).expect("saved questions encode");
    let running_limit = i64::from(before.context.limits.running_seconds) * 1_000_000_000;
    for (status, saved_questions, max_running) in [
        (
            "unrecognized_saved_state",
            questions.as_str(),
            running_limit,
        ),
        ("running", "{}", running_limit),
        ("running", questions.as_str(), 4_294_967_296_000_000_000),
    ] {
        sqlx::query("UPDATE fleet.tasks SET status=$2,questions=$3::jsonb,max_running_nanos=$4 WHERE id=$1::uuid")
            .bind(task.as_str()).bind(status).bind(saved_questions).bind(max_running)
            .execute(&mut *case.held.fixtures.database.acquire().await.expect("isolated storage connection"))
            .await.expect("schema-valid unreadable saved data");
        unreadable_result(case, task, before.context.objective.as_ref()).await;
    }
    sqlx::query("UPDATE fleet.tasks SET status=$2,questions=$3::jsonb,max_running_nanos=$4 WHERE id=$1::uuid")
        .bind(task.as_str()).bind(before.context.state.as_str()).bind(&questions).bind(running_limit)
        .execute(&mut *case.held.fixtures.database.acquire().await.expect("repair connection"))
        .await.expect("restore owned saved values");
    assert_eq!(
        tasks
            .read(scope, task)
            .await
            .expect("owner can recover the same work"),
        before
    );
    let replay = tasks
        .bind(case.lease(), &case.plan("retained-work", 1))
        .await
        .expect("repaired work replays");
    assert_eq!(replay.items, accepted.items);
    assert!(replay.replayed);
    assert_eq!(case.counts().await, (1, 1, 1));
}

async fn unreadable_result(case: &Case, task: &Uuid7, objective: &str) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let failure = tasks
        .read(scope, task)
        .await
        .expect_err("unreadable data must not look completed or missing");
    assert_eq!(failure.code(), error_code::INTERNAL_DB_QUERY);
    assert!(!failure.is_datastore_unavailable());
    assert_ne!(
        failure
            .source()
            .expect("decoder cause is retained")
            .to_string(),
        failure.to_string()
    );
    assert!(!failure.detail().contains(objective));
    assert_eq!(case.counts().await, (1, 1, 1));
}
