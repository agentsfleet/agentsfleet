//! Accepted items keep one original answer across replay and ownership checks.

use af_core::error_code;
use af_wire::task::{DecisionTaskVersion, TaskInputMode, TaskInputReceiptInput};
use afd_fleet::task::{TaskBindings, TaskLifecycle as _};

use super::fixture::{Case, exercise};

fn receipt(case: &Case, bindings: &TaskBindings, mode: TaskInputMode) -> TaskInputReceiptInput {
    let item = bindings.items.first().expect("accepted item");
    TaskInputReceiptInput {
        fencing_token: case.lease().fencing_token,
        operation_id: afd_db::test_util::mint_id(),
        source_admission_id: case.held.event_id.clone(),
        plan_digest: "a".repeat(64),
        item_id: item.item_id.to_string(),
        task: Some(DecisionTaskVersion {
            task_id: item.task_id.as_ref().expect("task owner").to_string(),
            generation: 1,
        }),
        mode,
        response_text: None,
    }
}

fn valid(input: TaskInputReceiptInput) -> garde::Valid<TaskInputReceiptInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded input receipt")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn queued_item_receipt_preserves_original_answer_on_replay_and_conflict() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.lease(), &case.plan(&"a".repeat(64), 1))
            .await
            .expect("accepted task");
        let input = receipt(case, &bindings, TaskInputMode::Queued);
        let accepted = case
            .tasks
            .accept_input(case.lease(), &valid(input.clone()))
            .await
            .expect("native queued acknowledgement");
        assert_eq!(accepted.mode, TaskInputMode::Queued);
        assert!(!accepted.replayed);
        assert_eq!(accepted.covered_input_sequence, Some(0));
        let replayed = case
            .tasks
            .accept_input(case.lease(), &valid(input.clone()))
            .await
            .expect("same acknowledgement replay");
        assert!(replayed.replayed);
        assert_eq!(replayed.item_id, accepted.item_id);
        conflicting_answer(case, &input).await;
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT response_text FROM fleet.task_input_receipts WHERE task_item_id=$1::uuid",
        )
        .bind(&input.item_id)
        .fetch_all(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("receipt read"),
        )
        .await
        .expect("stored original answer");
        assert_eq!(rows, ["Queued: Repair title decoding in PR 101"]);
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn input_receipts_refuse_changed_identity_generation_and_mode_without_writes() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.lease(), &case.plan(&"a".repeat(64), 1))
            .await
            .expect("accepted task");
        let original = receipt(case, &bindings, TaskInputMode::Answered);
        for (input, code) in invalid_inputs(&original) {
            let error = case
                .tasks
                .accept_input(case.lease(), &valid(input))
                .await
                .expect_err("invalid acknowledgement is refused");
            assert_eq!(error.code(), code);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM fleet.task_input_receipts WHERE task_item_id=$1::uuid",
        )
        .bind(original.item_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("receipt count"),
        )
        .await
        .expect("no partial receipt");
        assert_eq!(count, 0);
    })
    .await;
}

async fn conflicting_answer(case: &Case, input: &TaskInputReceiptInput) {
    let different = TaskInputReceiptInput {
        mode: TaskInputMode::Answered,
        operation_id: afd_db::test_util::mint_id(),
        response_text: Some("A conflicting replacement answer".to_owned()),
        ..input.clone()
    };
    let refused = case
        .tasks
        .accept_input(case.lease(), &valid(different))
        .await
        .expect_err("first answer remains authoritative");
    assert_eq!(refused.code(), error_code::TASK_REPLAY_CONFLICT);
}

fn invalid_inputs(
    original: &TaskInputReceiptInput,
) -> Vec<(TaskInputReceiptInput, error_code::ErrorCode)> {
    let mut wrong_plan = original.clone();
    wrong_plan.plan_digest = "b".repeat(64);
    let mut wrong_task = original.clone();
    wrong_task.task = None;
    let mut stale = original.clone();
    stale.task.as_mut().expect("task version").generation = 2;
    let mut queued_text = original.clone();
    queued_text.mode = TaskInputMode::Queued;
    queued_text.response_text = Some("invented queue acknowledgement".to_owned());
    let mut apply_text = queued_text.clone();
    apply_text.mode = TaskInputMode::ApplyHere;
    vec![
        (wrong_plan, error_code::TASK_NOT_FOUND),
        (wrong_task, error_code::TASK_INVALID_TARGET),
        (stale, error_code::TASK_STALE_STATE),
        (queued_text, error_code::TASK_INVALID_TARGET),
        (apply_text, error_code::TASK_REPLAY_CONFLICT),
        (original.clone(), error_code::TASK_INPUT_AMBIGUOUS),
    ]
}
