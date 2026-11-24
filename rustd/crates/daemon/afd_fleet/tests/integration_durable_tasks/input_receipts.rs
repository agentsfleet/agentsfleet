//! Accepted items keep one original answer across replay and ownership checks.

use af_core::error_code;
use af_wire::durable_task::{
    DecisionDurableTaskVersion, DurableTaskInputMode, DurableTaskInputReceiptInput,
};
use afd_fleet::durable_task::{DurableTaskBindings, DurableTaskLifecycle as _};

use super::fixture::{Case, exercise, id};

fn receipt(
    case: &Case,
    bindings: &DurableTaskBindings,
    mode: DurableTaskInputMode,
) -> DurableTaskInputReceiptInput {
    let item = bindings.items.first().expect("accepted item");
    DurableTaskInputReceiptInput {
        fencing_token: case.run().fencing_token,
        operation_id: afd_db::test_util::mint_id().into(),
        source_turn_id: case.held.turn_id.clone().into(),
        plan_digest: "a".repeat(64),
        item_id: item.item_id.to_string(),
        durable_task: Some(DecisionDurableTaskVersion {
            durable_task_id: item
                .durable_task_id
                .as_ref()
                .expect("task owner")
                .to_string(),
            generation: 1,
        }),
        mode,
        response_text: None,
    }
}

fn valid(input: DurableTaskInputReceiptInput) -> garde::Valid<DurableTaskInputReceiptInput> {
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
            .bind(case.run(), &case.plan(&"a".repeat(64), 1))
            .await
            .expect("accepted task");
        let input = receipt(case, &bindings, DurableTaskInputMode::Queued);
        let accepted = case
            .tasks
            .accept_input(case.run(), &valid(input.clone()))
            .await
            .expect("native queued acknowledgement");
        assert_eq!(accepted.mode, DurableTaskInputMode::Queued);
        assert!(!accepted.replayed);
        assert_eq!(accepted.covered_input_sequence, Some(0));
        let replayed = case
            .tasks
            .accept_input(case.run(), &valid(input.clone()))
            .await
            .expect("same acknowledgement replay");
        assert!(replayed.replayed);
        assert_eq!(replayed.item_id, accepted.item_id);
        conflicting_answer(case, &input).await;
        let rows: Vec<String> = sqlx::query_scalar(
            "SELECT response_text FROM fleet.durable_task_items WHERE id=$1 AND mode IS NOT NULL",
        )
        .bind(id(&input.item_id))
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
        // The source turn and the task's staged continuation; a receipt adds none.
        assert_eq!(case.counts().await, (1, 1, 2));
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn input_receipts_refuse_changed_identity_generation_and_mode_without_writes() {
    exercise(async |case| {
        let bindings = case
            .tasks
            .bind(case.run(), &case.plan(&"a".repeat(64), 1))
            .await
            .expect("accepted task");
        let original = receipt(case, &bindings, DurableTaskInputMode::Answered);
        for (input, code) in invalid_inputs(&original) {
            let error = case
                .tasks
                .accept_input(case.run(), &valid(input))
                .await
                .expect_err("invalid acknowledgement is refused");
            assert_eq!(error.code(), code);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT (SELECT count(*) FROM fleet.durable_task_items WHERE id=$1 AND mode IS NOT NULL) + (SELECT count(*) FROM fleet.durable_task_inputs WHERE durable_task_item_id=$1 AND operation_id IS NOT NULL)",
        )
        .bind(id(&original.item_id))
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

async fn conflicting_answer(case: &Case, input: &DurableTaskInputReceiptInput) {
    let different = DurableTaskInputReceiptInput {
        mode: DurableTaskInputMode::Answered,
        operation_id: afd_db::test_util::mint_id().into(),
        response_text: Some("A conflicting replacement answer".to_owned()),
        ..input.clone()
    };
    let refused = case
        .tasks
        .accept_input(case.run(), &valid(different))
        .await
        .expect_err("first answer remains authoritative");
    assert_eq!(refused.code(), error_code::DURABLE_TASK_REPLAY_CONFLICT);
}

fn invalid_inputs(
    original: &DurableTaskInputReceiptInput,
) -> Vec<(DurableTaskInputReceiptInput, error_code::ErrorCode)> {
    let mut wrong_plan = original.clone();
    wrong_plan.plan_digest = "b".repeat(64);
    let mut wrong_durable_task = original.clone();
    wrong_durable_task.durable_task = None;
    let mut stale = original.clone();
    stale
        .durable_task
        .as_mut()
        .expect("task version")
        .generation = 2;
    let mut queued_text = original.clone();
    queued_text.mode = DurableTaskInputMode::Queued;
    queued_text.response_text = Some("invented queue acknowledgement".to_owned());
    let mut apply_text = queued_text.clone();
    apply_text.mode = DurableTaskInputMode::ApplyHere;
    vec![
        (wrong_plan, error_code::DURABLE_TASK_NOT_FOUND),
        (wrong_durable_task, error_code::DURABLE_TASK_INVALID_TARGET),
        (stale, error_code::DURABLE_TASK_STALE_STATE),
        (queued_text, error_code::DURABLE_TASK_INVALID_TARGET),
        (apply_text, error_code::DURABLE_TASK_REPLAY_CONFLICT),
        (original.clone(), error_code::DURABLE_TASK_INPUT_AMBIGUOUS),
    ]
}
