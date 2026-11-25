//! A run's `task_input` joins a person's later words to an unfinished task,
//! waking it when it waits, and nothing else may.

use af_core::{error_code, id::Uuid7};
use af_wire::durable_task::{DurableTaskBindingItem, DurableTaskBindingsInput};
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use afd_turns::Producer;

use super::fixture::{Case, exercise};
use super::task_tools::{continuations, sealed, started, valid};

/// The `task_input` a run sends: the first word of `source` steers `task`.
fn task_input(case: &Case, source: &Uuid7, task: &str) -> DurableTaskBindingsInput {
    sealed(DurableTaskBindingsInput {
        fencing_token: case.run().fencing_token,
        source_turn_id: source.to_string(),
        plan_digest: "0".repeat(64),
        items: vec![DurableTaskBindingItem::Steer {
            source_start: 0,
            source_end: 6,
            durable_task_id: task.to_owned(),
        }],
    })
}

/// Runs one statement about `task`.
async fn alter(case: &Case, statement: &'static str, task: &Uuid7) {
    sqlx::query(statement)
        .bind(task)
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

/// Leaves `task` waiting, its continuation already run.
async fn waiting(case: &Case, task: &Uuid7) {
    alter(
        case,
        "UPDATE thread.turns SET settled_at = clock_timestamp() WHERE durable_task_id = $1",
        task,
    )
    .await;
    alter(
        case,
        "UPDATE fleet.durable_tasks SET status = 'waiting' WHERE id = $1",
        task,
    )
    .await;
}

/// The items bound to `task`, and its stored status.
async fn state(case: &Case, task: &Uuid7) -> (i64, String) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM fleet.durable_task_items WHERE durable_task_id = $1), \
         (SELECT status FROM fleet.durable_tasks WHERE id = $1)",
    )
    .bind(task)
    .fetch_one(
        &mut *case
            .held
            .fixtures
            .database
            .acquire()
            .await
            .expect("a connection"),
    )
    .await
    .expect("the task's items and status")
}

/// A person's later message, folded into the case's run.
async fn later_message(case: &Case, producer: Producer, actor: &str) -> Uuid7 {
    let source = case.source(producer, actor).await;
    case.assign(&source).await;
    source
}

// Value: protects=a person's later words wake a waiting task: one input under
// its generation and one continuation staged; fails_when=the steer leaves the
// task waiting or stages no continuation; seam=`DurableTasks::interpret` over
// live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_input_wakes_a_waiting_task() {
    exercise(async |case| {
        let task = started(case).await;
        waiting(case, &task).await;
        let (items, _) = state(case, &task).await;
        let person = afd_turns::view::steer_actor("native_durable_task_owner");
        let source = later_message(case, Producer::Steer, &person).await;

        case.tasks
            .interpret(case.run(), &valid(task_input(case, &source, task.as_str())))
            .await
            .expect("a person's words join the task");

        assert_eq!(state(case, &task).await, (items + 1, "ready".to_owned()));
        assert_eq!(
            continuations(case, &task).await,
            1,
            "one continuation staged"
        );
    })
    .await;
}

// Value: protects=only a person's words reach a task, and only a live task of
// the fleet; fails_when=a webhook's turn steers a task, or a finished or
// foreign task takes an input; seam=`DurableTasks::interpret` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_input_under_a_machine_turn_is_refused() {
    exercise(async |case| {
        let task = started(case).await;
        let (items, _) = state(case, &task).await;
        let person = afd_turns::view::steer_actor("native_durable_task_owner");
        let hook = later_message(case, Producer::Webhook, &person).await;
        let machine = case
            .tasks
            .interpret(case.run(), &valid(task_input(case, &hook, task.as_str())))
            .await;
        let spoken = later_message(case, Producer::Steer, &person).await;
        let foreign = afd_db::test_util::mint_id();
        let absent = case
            .tasks
            .interpret(case.run(), &valid(task_input(case, &spoken, foreign.as_str())))
            .await;
        alter(case, "UPDATE fleet.durable_tasks SET status = 'done', terminal_at = clock_timestamp() WHERE id = $1", &task).await;
        let finished = case
            .tasks
            .interpret(case.run(), &valid(task_input(case, &spoken, task.as_str())))
            .await;

        let code = |answer: afd_fleet::Result<_>| answer.err().map(|refusal: afd_fleet::Error| refusal.code());
        assert_eq!(code(machine), Some(error_code::DURABLE_TASK_INVALID_TARGET));
        assert_eq!(code(absent), Some(error_code::DURABLE_TASK_NOT_FOUND));
        assert_eq!(code(finished), Some(error_code::DURABLE_TASK_NOT_FOUND));
        assert_eq!(state(case, &task).await.0, items, "no input row");
    })
    .await;
}
