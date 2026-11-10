//! A live run never folds a message that sits behind a task-bound one.

use afd_admission::Producer;
use afd_fleet::task::TaskLifecycle as _;

use super::fixture::{Case, exercise};

/// Binding a plan queues its continuations as task-bound messages after the
/// run's own. A plain run's claim takes what was sent before them and stops
/// at the first, as it stops at a parked message.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_bound_message_stops_a_live_runs_fold() {
    exercise(claim_around_bound_continuations).await;
}

async fn claim_around_bound_continuations(case: &Case) {
    let held = &case.held;
    let actor = afd_events::steer_actor("native_task_owner");
    let before = case
        .source_message(Producer::Steer, &actor, "check the logs first")
        .await;
    case.tasks
        .bind(case.lease(), &case.plan("three-goals", 3))
        .await
        .expect("the plan binds and queues its continuations");
    let after = case
        .source_message(Producer::Steer, &actor, "and the dashboard")
        .await;
    assert!(
        waiting_task_bound(case).await > 0,
        "task-bound continuations wait, unparked, behind the run"
    );
    // The bind made this lease a durable task's, which claims nothing. Making
    // it a plain run again leaves the bound continuations as the only thing
    // that can stop the claim.
    sqlx::query("UPDATE runner.leases SET task_id = NULL WHERE id = $1")
        .bind(&held.issued.lease_id)
        .execute(&mut *held.fixtures.database.acquire().await.expect("connection"))
        .await
        .expect("the lease becomes a plain run");
    let claimed = held
        .leases
        .claim_folds(&held.runner, &held.issued.lease_id, held.fence.as_u64(), 0)
        .await
        .expect("the claim reaches the datastore");
    let ids: Vec<&str> = claimed
        .inputs
        .iter()
        .map(|input| input.event_id.as_ref())
        .collect();
    assert_eq!(
        ids,
        [before.as_str()],
        "{} waits behind the task-bound messages",
        after.as_str()
    );
}

/// The unsettled, unparked, task-bound messages after the run's own.
async fn waiting_task_bound(case: &Case) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM fleet.admissions a WHERE a.fleet_id = $1 \
         AND a.task_id IS NOT NULL AND a.parked_at IS NULL AND a.settled_at IS NULL \
         AND a.sequence > (SELECT sequence FROM fleet.admissions WHERE id = $2)",
    )
    .bind(&case.scope.fleet_id)
    .bind(&case.held.event_id)
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
