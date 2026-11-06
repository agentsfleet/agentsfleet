//! An issued continuation whose task generation moves leaves the fleet's
//! backlog count at once (`M242_001` Dimension 1.8).

use afd_fleet::task::{TaskControl, TaskLifecycle as _};

use super::fixture::{Case, exercise};

/// Marks the task's unsettled continuation issued: an event row named by the
/// admission, and a reported lease on it, copied from the held lease.
const ISSUE: &str = "WITH c AS (SELECT id FROM fleet.admissions \
    WHERE task_id = $2::uuid AND producer = 'task_continuation' AND settled_at IS NULL), \
    e AS (INSERT INTO fleet.events \
    (fleet_id,workspace_id,event_id,actor,event_type,status,request_json,created_at,updated_at) \
    SELECT l.fleet_id,l.workspace_id,c.id::text,'fixture','chat','processed','{}',now(),now() \
    FROM runner.leases l CROSS JOIN c WHERE l.id = $1::uuid RETURNING id) \
    INSERT INTO runner.leases (id,runner_id,fleet_id,workspace_id,tenant_id,fleet_event_id, \
    event_created_at,posture,provider,model,last_metered_at,fencing_token,lease_expires_at, \
    status,posted_message_count,created_at,updated_at) \
    SELECT uuidv7(),l.runner_id,l.fleet_id,l.workspace_id,l.tenant_id,e.id,l.event_created_at, \
    l.posture,l.provider,l.model,l.last_metered_at,0,l.lease_expires_at,$3,0,now(),now() \
    FROM runner.leases l CROSS JOIN e WHERE l.id = $1::uuid RETURNING fleet_event_id::text";

/// The continuation's settle state, the fleet's stored count, and the count
/// #803's budget rule gives over the same rows.
const STATE: &str = "SELECT \
    (SELECT bool_and(settled_at IS NOT NULL) FROM fleet.admissions \
       WHERE task_id = $2::uuid AND producer = 'task_continuation'), \
    (SELECT unsettled_count FROM fleet.admission_backlogs WHERE fleet_id = $1::uuid), \
    (SELECT count(*) FROM fleet.admissions a WHERE a.fleet_id = $1::uuid AND a.settled_at IS NULL \
       AND (a.producer <> 'task_continuation' OR a.task_generation IS NULL OR EXISTS ( \
         SELECT 1 FROM fleet.tasks t WHERE t.id = a.task_id AND t.generation = a.task_generation \
           AND t.terminal_at IS NULL)))";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_issued_obsolete_continuation_is_settled_and_released() {
    exercise(issued_then_cancelled).await;
}

async fn issued_then_cancelled(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let bindings = tasks
        .bind(case.lease(), &case.plan("obsolete-continuation", 1))
        .await
        .expect("one goal");
    let task = bindings
        .items
        .first()
        .and_then(|item| item.task_id.as_ref())
        .expect("accepted work owns a task");
    let initial = tasks.read(scope, task).await.expect("current goal");
    let paused = tasks
        .control(
            scope,
            task,
            &initial.etag(),
            initial.context.generation,
            TaskControl::Pause,
        )
        .await
        .expect("pause");
    let resumed = tasks
        .control(
            scope,
            task,
            &paused.etag(),
            paused.context.generation,
            TaskControl::Resume,
        )
        .await
        .expect("resume enqueues one continuation");
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("fixture");
    let issued: Option<String> = sqlx::query_scalar(ISSUE)
        .bind(case.held.lease_key.as_str())
        .bind(task.as_str())
        .bind(afd_state::sql::LEASE_STATUS_REPORTED)
        .fetch_optional(&mut *connection)
        .await
        .expect("issue the continuation");
    assert!(issued.is_some(), "the resumed task holds one continuation");
    let before: i64 = sqlx::query_scalar(
        "SELECT unsettled_count FROM fleet.admission_backlogs WHERE fleet_id = $1::uuid",
    )
    .bind(scope.fleet_id.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("count before the advance");
    drop(connection);
    tasks
        .control(
            scope,
            task,
            &resumed.etag(),
            resumed.context.generation,
            TaskControl::Cancel,
        )
        .await
        .expect("cancel advances the generation");
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("observer");
    let (settled, stored, rule): (Option<bool>, i64, i64) = sqlx::query_as(STATE)
        .bind(scope.fleet_id.as_str())
        .bind(task.as_str())
        .fetch_one(&mut *connection)
        .await
        .expect("read the count");
    assert_eq!(settled, Some(true), "the issued continuation is settled");
    assert_eq!(
        stored,
        before - 1,
        "the count drops by the one continuation"
    );
    assert_eq!(stored, rule, "the count equals #803's budget rule");
}
