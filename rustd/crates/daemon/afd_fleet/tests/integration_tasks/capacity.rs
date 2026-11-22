//! A capacity failure rolls back every item, goal and queued continuation.
//!
//! Unset, `max_unfinished_per_fleet` limits nothing, so the deployment here
//! sets it to [`UNFINISHED`].

use af_core::error_code;
use af_wire::task::TaskLimits;
use afd_fleet::task::policy::{TaskPolicy, TaskSettings};
use afd_fleet::task::{TaskBindings, TaskControl, TaskDependencies, TaskLifecycle as _, Tasks};

use super::fixture::{Case, exercise};

/// The deployment's unfinished-task setting under test.
const UNFINISHED: u32 = 16;

/// The case's task service under a deployment that sets [`UNFINISHED`].
fn configured(case: &Case) -> Tasks {
    let settings = TaskSettings {
        max_unfinished_per_fleet: UNFINISHED,
        ..TaskSettings::default()
    };
    let checked = garde::Unvalidated::new(settings)
        .validate()
        .expect("a positive setting");
    let dependencies = TaskDependencies {
        policy: TaskPolicy::new(&checked),
        ..TaskDependencies::from(&case.held.fixtures.plane())
    };
    Tasks::new(case.held.runs.clone(), case.turns.clone(), dependencies)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_partially_full_fleet_rolls_back_the_whole_plan_and_retries_after_room_opens() {
    exercise(capacity).await;
}

async fn fill_capacity(case: &Case, tasks: &Tasks) -> TaskBindings {
    let first = tasks
        .bind(case.run(), &case.plan("first-three", 3))
        .await
        .expect("first goals");
    for round in 1..5 {
        let source = case.assigned_source().await;
        let plan = case.source_plan(
            source,
            &format!("more-goals-{round}"),
            3,
            TaskLimits::default(),
        );
        let reply = tasks
            .bind(case.run(), &plan)
            .await
            .expect("more queued goals");
        assert_eq!(reply.items.len(), 3);
        assert!(!reply.replayed);
    }
    first
}

async fn capacity(case: &Case) {
    let configured = configured(case);
    let (tasks, scope) = (&configured, &case.scope);
    let first = fill_capacity(case, tasks).await;
    let before = case.counts().await;
    assert_eq!((before.0, before.1), (15, 15));
    let source = case.assigned_source().await;
    let plan = case.source_plan(source, "two-at-capacity", 2, TaskLimits::default());
    let with_source = case.counts().await;
    assert_eq!(with_source, (before.0, before.1, before.2 + 1));
    let refusal = tasks
        .bind(case.run(), &plan)
        .await
        .expect_err("second item exceeds the last remaining slot");
    assert_eq!(refusal.code(), error_code::TASK_LIMIT_EXHAUSTED);
    assert_eq!(
        case.counts().await,
        with_source,
        "no partial task, item or continuation survives"
    );

    let queued = first
        .items
        .get(1)
        .expect("a queued accepted task")
        .task_id
        .as_ref()
        .expect("accepted new-work item owns a task");
    let queued_snapshot = tasks.read(scope, queued).await.expect("queued goal");
    tasks
        .control(
            scope,
            queued,
            &queued_snapshot.etag(),
            queued_snapshot.context.generation,
            TaskControl::Cancel,
        )
        .await
        .expect("release one unfinished slot");
    let retry = tasks
        .bind(case.run(), &plan)
        .await
        .expect("same complete interpretation now fits");
    assert_eq!(retry.items.len(), 2);
    assert!(!retry.replayed);
    assert_eq!(case.counts().await, (17, 17, with_source.2 + 2));
    recovered_plan(case, tasks, &plan, &retry, with_source.2 + 2).await;
}

async fn recovered_plan(
    case: &Case,
    tasks: &Tasks,
    plan: &afd_fleet::task::PreparedPlan,
    retry: &TaskBindings,
    turns: i64,
) {
    let scope = &case.scope;
    let replay = tasks
        .bind(case.run(), plan)
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
    assert_eq!(case.counts().await, (17, 17, turns));
}
