//! A queued acknowledgement names a live task, and one operation id names one
//! acknowledgement.

use af_core::{error_code, id::Uuid7};
use af_wire::task::{DecisionTaskVersion, TaskInputMode, TaskInputReceiptInput};
use afd_fleet::task::{TaskBinding, TaskLifecycle as _};

use super::fixture::{Case, exercise};

fn queued(
    case: &Case,
    item: &TaskBinding,
    operation: &Uuid7,
) -> garde::Valid<TaskInputReceiptInput> {
    garde::Unvalidated::new(TaskInputReceiptInput {
        fencing_token: case.run().fencing_token,
        operation_id: operation.to_string(),
        source_turn_id: case.held.turn_id.clone().into(),
        plan_digest: "a".repeat(64),
        item_id: item.item_id.to_string(),
        task: Some(DecisionTaskVersion {
            task_id: item.task_id.as_ref().expect("task owner").to_string(),
            generation: 1,
        }),
        mode: TaskInputMode::Queued,
        response_text: None,
    })
    .validate()
    .expect("bounded input receipt")
}

async fn receipts(case: &Case) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT response_text FROM fleet.task_input_receipts WHERE fleet_id=$1 ORDER BY response_text",
    )
    .bind(&case.scope.fleet_id)
    .fetch_all(&mut *case.held.fixtures.database.acquire().await.expect("receipt observer"))
    .await
    .expect("stored receipts")
}

// Value: protects=a finished task is never acknowledged as queued work, and the
// refusal stores nothing; fails_when=the terminal check is dropped and a done
// task is answered "Queued"; why_new=the terminal arm of the queued receipt had
// no case; seam=the production receipt over live Postgres, the item's task done.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_queued_receipt_for_a_finished_task_is_refused_as_stale() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.run(), &case.plan(&"a".repeat(64), 1))
            .await
            .expect("accepted task");
        let item = bindings.items.first().expect("accepted item");
        let task = item.task_id.as_ref().expect("task owner");
        let done = sqlx::query(
            "UPDATE fleet.tasks SET status='done', terminal_at=clock_timestamp() WHERE id=$1",
        )
        .bind(task)
        .execute(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("task connection"),
        )
        .await
        .expect("the task finishes");
        assert_eq!(done.rows_affected(), 1);

        let refused = case
            .tasks
            .accept_input(
                case.run(),
                &queued(case, item, &afd_db::test_util::mint_id()),
            )
            .await
            .expect_err("a done task is not queued work");

        assert_eq!(refused.code(), error_code::TASK_STALE_STATE);
        assert_eq!(
            receipts(case).await,
            Vec::<String>::new(),
            "nothing is stored"
        );
    })
    .await;
}

// Value: protects=an operation id already spent on one acknowledgement cannot be
// replayed onto another item's receipt; fails_when=the replay read takes the
// first of two matching receipts and answers it as this one's replay;
// why_new=the two-receipt arm of the replay read had no case; seam=the production
// receipt over live Postgres, two items acknowledged under their own operations.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_operation_id_reused_across_two_receipts_is_a_replay_conflict() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.run(), &case.plan(&"a".repeat(64), 2))
            .await
            .expect("accepted tasks");
        let mut items = bindings.items.iter();
        let first = items.next().expect("the first accepted item");
        let second = items.next().expect("the second accepted item");
        let (spent, own) = (afd_db::test_util::mint_id(), afd_db::test_util::mint_id());
        for (item, operation) in [(first, &spent), (second, &own)] {
            case.tasks
                .accept_input(case.run(), &queued(case, item, operation))
                .await
                .expect("each item is acknowledged once");
        }
        let before = receipts(case).await;
        assert_eq!(before.len(), 2);

        let refused = case
            .tasks
            .accept_input(case.run(), &queued(case, second, &spent))
            .await
            .expect_err("another receipt's operation is not this one's replay");

        assert_eq!(refused.code(), error_code::TASK_REPLAY_CONFLICT);
        assert_eq!(receipts(case).await, before, "nothing is stored");
    })
    .await;
}
