//! A completed HTTP read cannot revive work after its native authority changes.
use super::{Observation, Waiting, baseline, counts, exercise, id, observe, wait_state};
use afd_fleet::task::{TaskControl, TaskLifecycle as _};
use afd_wire::task::TaskState;
use std::time::Duration;

#[derive(Clone, Copy)]
enum Change {
    Pause,
    Revoke,
    Spent,
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn completed_read_cannot_bypass_pause_revocation_or_spent_allowance_after_claim() {
    for change in [Change::Pause, Change::Revoke, Change::Spent] {
        exercise(async |case| {
            let waiting = Box::pin(Waiting::create(case)).await;
            let before = baseline(&waiting).await;
            waiting.due().await;
            race(&waiting, change).await;
            assert_eq!(
                baseline(&waiting).await,
                before,
                "unusable observations do not replace retained evidence"
            );
            assert_eq!(wait_state(&waiting).await, (false, true, true, true, 0));
            assert_eq!(counts(case, &waiting.task.task_id).await, (0, 0, 0, 2, 1));
            assert_eq!(waiting.fixture.counts().await, (1, 1, 1));
            assert_stopped(&waiting, change).await;
            waiting.finish().await;
        })
        .await;
    }
}

async fn race(waiting: &Waiting<'_>, change: Change) {
    let (entered, release) = waiting
        .server
        .hold_response("/repos/example/linkwarden/pulls/101");
    observe(&waiting.server, Observation::Success);
    let probe = waiting.tasks.maintain_waits();
    tokio::pin!(probe);
    tokio::time::timeout(Duration::from_secs(5),async {
        tokio::select! {
            result=&mut probe => panic!("maintenance finished before the physical response barrier: {result:?}"),
            entered=entered => entered.expect("committed claim reached the external HTTP read"),
        }
    }).await.expect("bounded HTTP claim barrier");
    assert_eq!(wait_state(waiting).await, (false, false, true, true, 0));
    change_authority(waiting, change).await;
    release
        .send(())
        .expect("native change committed before response release");
    let result = probe.await.expect("late evidence is safely refused");
    assert_eq!((result.checked, result.woken, result.deferred), (1, 0, 1));
}

async fn change_authority(waiting: &Waiting<'_>, change: Change) {
    let case = waiting.fixture.case;
    if matches!(change, Change::Pause) {
        let task = id(&waiting.task.task_id);
        let before = waiting
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("waiting task");
        waiting
            .tasks
            .control(
                &case.scope,
                &task,
                &before.etag(),
                before.context.generation,
                TaskControl::Pause,
            )
            .await
            .expect("human pause commits while network is outstanding");
        return;
    }
    let (statement, owner) = match change {
        Change::Revoke => (
            "UPDATE connector.integration_grants SET status='revoked',revoked_at=clock_timestamp() WHERE fleet_id=$1::uuid AND service='github'",
            case.held.fleet.as_str(),
        ),
        Change::Spent => (
            "UPDATE fleet.tasks SET used_model_nanos=max_model_nanos WHERE id=$1::uuid",
            waiting.task.task_id.as_str(),
        ),
        Change::Pause => unreachable!("pause used the public control verb"),
    };
    let changed = sqlx::query(statement)
        .bind(owner)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("concurrent native change"),
        )
        .await
        .expect("preexisting authority or late measured usage changed");
    assert_eq!(changed.rows_affected(), 1);
}

async fn assert_stopped(waiting: &Waiting<'_>, change: Change) {
    let case = waiting.fixture.case;
    let actual: (String, Option<String>, i64, i64) = sqlx::query_as(
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
            .expect("post-race observer"),
    )
    .await
    .expect("committed task state after late evidence");
    let expected = match change {
        Change::Pause => (TaskState::Paused, Some("validation_pending")),
        Change::Revoke => (TaskState::Blocked, Some("dependency_access_unavailable")),
        Change::Spent => (TaskState::Exhausted, Some("cumulative_limit_reached")),
    };
    assert_eq!(
        actual,
        (
            expected.0.as_str().to_owned(),
            expected.1.map(str::to_owned),
            3,
            2
        )
    );
}
