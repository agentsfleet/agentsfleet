//! A capacity failure rolls back every item, goal and queued continuation.

use afd_core::error_code;
use afd_fleet::task::{TaskBindings, TaskControl, TaskLifecycle as _};
use afd_wire::task::TaskLimits;

use super::fixture::{Case, exercise};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_partially_full_fleet_rolls_back_the_whole_plan_and_retries_after_room_opens() {
    exercise(capacity).await;
}

async fn fill_capacity(case: &Case) -> TaskBindings {
    let tasks = &case.tasks;
    let first = tasks
        .bind(case.lease(), &case.plan("first-three", 3))
        .await
        .expect("first goals");
    for round in 1..5 {
        let source = case.claimed_source().await;
        let plan = case.source_plan(
            source,
            &format!("more-goals-{round}"),
            3,
            TaskLimits::default(),
        );
        let reply = tasks
            .bind(case.lease(), &plan)
            .await
            .expect("more queued goals");
        assert_eq!(reply.items.len(), 3);
        assert!(!reply.replayed);
    }
    first
}

async fn capacity(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let first = fill_capacity(case).await;
    let before = case.counts().await;
    assert_eq!((before.0, before.1), (15, 15));
    let source = case.claimed_source().await;
    let plan = case.source_plan(source, "two-at-capacity", 2, TaskLimits::default());
    let with_source = case.counts().await;
    assert_eq!(with_source, (before.0, before.1, before.2 + 1));
    let refusal = tasks
        .bind(case.lease(), &plan)
        .await
        .expect_err("second item exceeds the last remaining slot");
    assert_eq!(refusal.code(), error_code::TASK_LIMIT_EXHAUSTED);
    assert_eq!(
        case.counts().await,
        with_source,
        "no partial task, item or continuation survives"
    );

    let queued = &first.items.get(1).expect("a queued accepted task").task_id;
    let queued_snapshot = tasks.read(scope, queued).await.expect("queued goal");
    tasks
        .control(
            scope,
            queued,
            queued_snapshot.resource_version,
            TaskControl::Cancel,
        )
        .await
        .expect("release one unfinished slot");
    let retry = tasks
        .bind(case.lease(), &plan)
        .await
        .expect("same complete interpretation now fits");
    assert_eq!(retry.items.len(), 2);
    assert!(!retry.replayed);
    assert_eq!(case.counts().await, (17, 17, with_source.2 + 2));
    recovered_plan(case, &plan, &retry, with_source.2 + 2).await;
}

async fn recovered_plan(
    case: &Case,
    plan: &afd_fleet::task::PreparedPlan,
    retry: &TaskBindings,
    admissions: i64,
) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let replay = tasks
        .bind(case.lease(), plan)
        .await
        .expect("stable replay after rollback and retry");
    assert!(replay.replayed);
    assert_eq!(replay.items, retry.items);
    assert_eq!(
        tasks
            .list(scope, None, 16)
            .await
            .expect("capacity read")
            .items
            .len(),
        16
    );
    assert_eq!(case.counts().await, (17, 17, admissions));
}
