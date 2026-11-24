//! A live run never folds a message that sits behind a task-bound one.

use afd_fleet::durable_task::DurableTaskLifecycle as _;
use afd_turns::Producer;

use super::fixture::{Case, exercise};

/// Binding a plan queues its continuations as task-bound messages after the
/// run's own. A plain run's assignment takes what was sent before them and stops
/// at the first, as it stops at a parked message.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_durable_task_bound_message_stops_a_live_runs_fold() {
    exercise(assign_around_bound_continuations).await;
}

async fn assign_around_bound_continuations(case: &Case) {
    let held = &case.held;
    let actor = afd_turns::view::steer_actor("native_durable_task_owner");
    let before = case
        .source_message(Producer::Steer, &actor, "check the logs first")
        .await;
    let bound = case
        .tasks
        .bind(case.run(), &case.plan("three-goals", 3))
        .await
        .expect("the plan binds and queues its continuations");
    let after = case
        .source_message(Producer::Steer, &actor, "and the dashboard")
        .await;
    assert_eq!(
        waiting_durable_task_bound(case).await,
        3,
        "each task's continuation waits, unparked, behind the test_run"
    );
    task_run_folds_nothing(case, &bound).await;
    // Making the run a plain one again leaves the bound continuations as the
    // only thing that can stop the assignment.
    clear_durable_task_binding(case).await;
    let assigned = held
        .runs
        .assign_folds(&held.runner, &held.issued.run_id, held.fence.as_u64(), 0)
        .await
        .expect("the assignment reaches the datastore");
    let ids: Vec<&str> = assigned
        .inputs
        .iter()
        .map(|input| input.turn_id.as_ref())
        .collect();
    assert_eq!(
        ids,
        [before.as_str()],
        "{} waits behind the task-bound messages",
        after.as_str()
    );
}

/// A durable task's run is assigned nothing: the fixture run takes the first
/// task, as its continuation's run would.
async fn task_run_folds_nothing(case: &Case, bound: &afd_fleet::durable_task::DurableTaskBindings) {
    let held = &case.held;
    let first = bound
        .items
        .first()
        .and_then(|item| item.durable_task_id.as_ref())
        .expect("the first goal owns a task");
    case.attach(first).await;
    assert_eq!(
        held.runs
            .assign_folds(&held.runner, &held.issued.run_id, held.fence.as_u64(), 0)
            .await
            .expect("bound task fold refusal")
            .inputs
            .len(),
        0
    );
}

async fn clear_durable_task_binding(case: &Case) {
    // Both immutable generation witnesses distinguish task work from ordinary chat.
    sqlx::query(
        "WITH reset AS (UPDATE thread.runs SET durable_task_id = NULL, durable_task_generation = NULL, \
         durable_task_stop_generation = NULL, durable_task_started_at = NULL, durable_task_runtime_accounted_at = NULL, \
         durable_task_execution_deadline_at = NULL WHERE id = $1 RETURNING turn_id) \
         UPDATE thread.turns a SET durable_task_id = NULL, durable_task_generation = NULL \
         FROM reset WHERE a.id = reset.turn_id",
    )
    .bind(&case.held.issued.run_id)
    .execute(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("connection"),
    )
    .await
    .expect("the run and its turn become an ordinary test_run");
}

/// The unsettled, unparked, task-bound messages after the run's own.
async fn waiting_durable_task_bound(case: &Case) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns a WHERE a.thread_id = $1 \
         AND a.durable_task_id IS NOT NULL AND a.parked_at IS NULL AND a.settled_at IS NULL \
         AND a.sequence > (SELECT sequence FROM thread.turns WHERE id = $2)",
    )
    .bind(&case.scope.fleet_id)
    .bind(&case.held.turn_id)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("connection"),
    )
    .await
    .expect("the waiting messages are countable")
}
