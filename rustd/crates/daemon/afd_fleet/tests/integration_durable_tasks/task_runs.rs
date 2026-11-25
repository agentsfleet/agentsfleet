//! A started task's continuation becomes the task's own run; a plain message
//! stays a plain run; an approval mid-task does not hold the task back.

use af_core::id::Uuid7;
use afd_fleet::run::{Billed, Issued};
use afd_turns::Producer;
use serde_json::Value;

use super::fixture::{Case, exercise};
use super::task_tools::{decision_calls, started};
use crate::seed::{MODEL, PROVIDER, select_fleet};

/// Runs one statement about `id`.
async fn alter(case: &Case, statement: &'static str, id: &Uuid7) {
    sqlx::query(statement)
        .bind(id)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the fixture statement runs");
}

/// Settles the case's head turn and frees its thread, as a finished run
/// leaves them, so the fleet's next head is assigned.
async fn finish_head(case: &Case) {
    alter(
        case,
        "UPDATE thread.turns SET settled_at = clock_timestamp() WHERE id = $1",
        &case.held.turn_id,
    )
    .await;
    alter(
        case,
        "UPDATE thread.runs SET status = 'expired' WHERE fleet_id = $1 AND status = 'active'",
        &case.held.fleet,
    )
    .await;
    alter(
        case,
        "UPDATE thread.threads SET run_expires_at = clock_timestamp() - interval '1 second', \
         hold_expires_at = NULL WHERE fleet_id = $1",
        &case.held.fleet,
    )
    .await;
}

/// Assigns the fleet's next head to the case's runner and issues its run.
async fn issue_next(case: &Case) -> (Uuid7, Issued) {
    let acquired = select_fleet(&case.held.runs, &case.held.runner, &case.held.fleet)
        .await
        .expect("the fleet's next head is assigned");
    let issued = case
        .held
        .runs
        .issue(
            &case.held.runner,
            &acquired,
            Billed {
                tenant_id: &case.held.tenant,
                posture: afd_billing::sql::posture::PLATFORM,
                provider: PROVIDER,
                model: MODEL,
            },
            case.held.now,
        )
        .await
        .expect("the issue answers")
        .expect("the head is issued");
    (acquired.turn_id, issued)
}

/// Assigns the fleet's next head to the case's runner and delivers it through
/// the gate chain, as a runner's poll does.
async fn deliver_next(case: &Case) -> Value {
    let acquired = select_fleet(&case.held.runs, &case.held.runner, &case.held.fleet)
        .await
        .expect("the fleet's next head is assigned");
    case.held
        .fixtures
        .plane()
        .assign_selected(acquired, &case.held.runner, af_core::clock::now())
        .await
        .expect("every gate verdict is a decision, not a fault")
        .expect("the fleet's next head is delivered")
}

/// The task the issued run is bound to.
fn bound(issued: &Issued) -> Option<String> {
    issued
        .task
        .as_ref()
        .map(|task| task.durable_task_id.to_string())
}

// Value: protects=a started task's continuation is delivered as the task's
// own run, offered the published catalog; fails_when=the continuation runs
// unbound, is held behind a check that no longer exists, or a task run loses
// its tools; seam=assignment and the gate chain's delivery over live Postgres
// and NATS.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_continuation_runs_with_tools() {
    exercise(async |case| {
        let task = started(case).await;
        finish_head(case).await;
        let fixtures = &case.held.fixtures;
        let _default = crate::integration_run_gates::seed::seed_provider_resolution(
            fixtures,
            &case.held.fleet,
        )
        .await;

        let run = deliver_next(case).await;

        let delivered = run.pointer("/durable_task/durable_task_id");
        assert_eq!(delivered.and_then(Value::as_str), Some(task.as_str()));
        let run_id = run.get("run_id").and_then(Value::as_str);
        let run_id = Uuid7::parse(run_id.expect("the run's id")).expect("an issued run id");
        let held: Option<Uuid7> =
            sqlx::query_scalar("SELECT durable_task_id FROM thread.runs WHERE id = $1")
                .bind(&run_id)
                .fetch_one(&mut *fixtures.database.acquire().await.expect("a connection"))
                .await
                .expect("the delivered run");
        assert_eq!(held, Some(task), "the run row is bound to the task");
        let tools: Vec<&str> = run
            .pointer("/policy/tools")
            .and_then(Value::as_array)
            .expect("the offered tools")
            .iter()
            .filter_map(Value::as_str)
            .collect();
        let catalog = afd_gate::policy::catalog::offered(false);
        assert!(
            catalog.iter().all(|tool| tools.contains(&tool.as_ref())),
            "the task's run is offered the published catalog: {tools:?}"
        );
    })
    .await;
}

// Value: protects=a person's plain message on a fleet with a waiting task
// runs as a plain run and makes no decision call; fails_when=the plain run is
// bound to the task, or its assignment asks the decision model; seam=
// assignment and issue over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_plain_message_makes_no_decision_call() {
    exercise(async |case| {
        let task = started(case).await;
        alter(
            case,
            "UPDATE thread.turns SET settled_at = clock_timestamp() WHERE durable_task_id = $1",
            &task,
        )
        .await;
        alter(
            case,
            "UPDATE fleet.durable_tasks SET status = 'waiting' WHERE id = $1",
            &task,
        )
        .await;
        finish_head(case).await;
        let person = afd_turns::view::steer_actor("native_durable_task_owner");
        let message = case.source(Producer::Steer, &person).await;

        let (turn, issued) = issue_next(case).await;

        assert_eq!(turn, message);
        assert_eq!(bound(&issued), None, "a plain message runs unbound");
        assert_eq!(decision_calls(case).await, 0);
    })
    .await;
}

// Value: protects=a grant a person approves while a task waits to run does
// not hold the task back; fails_when=a change to the fleet's access blocks
// the task's next run, as a changed setup once did; seam=assignment and issue
// over live Postgres, a secret grant approved between the start and the run.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_continues_after_a_grant_is_approved() {
    exercise(async |case| {
        let task = started(case).await;
        case.held
            .fixtures
            .approve_secret(&case.held.fleet, "sentry")
            .await;
        finish_head(case).await;

        let (_turn, issued) = issue_next(case).await;

        assert_eq!(bound(&issued).as_deref(), Some(task.as_str()));
    })
    .await;
}
