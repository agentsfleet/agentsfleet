//! Safe-boundary input recovery preserves FIFO and paid accepted meaning.
use super::{
    super::fixture::{Case, GOALS, exercise},
    existing_inputs::proposed,
    fixture::Decisions,
};
use af_core::{error_code, paging::Cursor};
use af_wire::task::{TaskInputAction, TaskInputMode, TaskInputReceiptInput, TaskInputsQuery};
use afd_fleet::task::TaskLifecycle as _;

fn query(case: &Case, cursor: Option<String>, after: Option<u64>) -> garde::Valid<TaskInputsQuery> {
    garde::Unvalidated::new(TaskInputsQuery {
        fencing_token: case.lease().fencing_token,
        starting_after: cursor,
        after_input_sequence: after,
        limit: 1,
    })
    .validate()
    .expect("bounded safe-boundary read")
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn human_input_pages_keep_complete_fifo_messages_without_claiming_or_consuming() {
    exercise(async |case| {
        let actor = afd_events::steer_actor("native_task_owner");
        let first = case.source_message(afd_admission::Producer::Steer, &actor, "First: café 🦉").await;
        let second = case.source_message(afd_admission::Producer::Steer, &actor, "Second: preserve middleware").await;
        let page = case.tasks.inputs(case.lease(), &query(case, None, None)).await.expect("first FIFO page");
        assert_eq!(page.source_admission_id, case.held.event_id);
        assert_eq!(page.current_message.as_deref(), Some(GOALS.join("; ").as_str()));
        assert_eq!(page.inputs.len(), 1);
        let item = page.inputs.first().expect("first human message");
        assert_eq!(item.source_admission_id, first.as_str());
        assert_eq!(item.message, "First: café 🦉");
        assert!(item.accepted.is_none());
        let cursor = page.next_cursor.expect("second input remains");
        assert_eq!(cursor, Cursor::Text { value: item.sequence.to_string(), id: first.to_string() }.to_string());
        let next = case.tasks.inputs(case.lease(), &query(case, Some(cursor), None)).await.expect("second FIFO page");
        assert_eq!(next.inputs.len(), 1);
        let later = next.inputs.first().expect("second human message");
        assert_eq!(later.source_admission_id, second.as_str());
        assert_eq!(later.message, "Second: preserve middleware");
        assert!(later.sequence > item.sequence);
        assert!(next.next_cursor.is_none());
        let count: i64 = sqlx::query_scalar("SELECT count(*) FROM fleet.admissions WHERE id IN ($1,$2) AND settled_at IS NULL AND input_claim_lease_id IS NULL")
            .bind(first).bind(second).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("read-only ownership observer")).await.expect("unconsumed inputs");
        assert_eq!(count, 2);
    }).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_input_cursor_refuses_without_claiming_a_human_source() {
    exercise(async |case| {
        let source = case.source(afd_admission::Producer::Steer, &afd_events::steer_actor("native_task_owner")).await;
        for cursor in invalid_cursors(source.as_str()) {
            let error = case.tasks.inputs(case.lease(), &query(case, Some(cursor), None)).await.expect_err("invalid input boundary");
            assert_eq!(error.code(), error_code::INVALID_REQUEST);
        }
        let unclaimed: bool = sqlx::query_scalar("SELECT input_claim_lease_id IS NULL AND settled_at IS NULL FROM fleet.admissions WHERE id=$1")
            .bind(source).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("source observer")).await.expect("unchanged human input");
        assert!(unclaimed);
    }).await;
}

fn invalid_cursors(id: &str) -> Vec<String> {
    vec![
        "invalid cursor".to_owned(),
        Cursor::Timestamp {
            at: af_core::clock::now(),
            id: id.to_owned(),
        }
        .to_string(),
        Cursor::Text {
            value: "1".to_owned(),
            id: "not-an-id".to_owned(),
        }
        .to_string(),
        Cursor::Text {
            value: "not-a-number".to_owned(),
            id: id.to_owned(),
        }
        .to_string(),
        Cursor::Text {
            value: "-1".to_owned(),
            id: id.to_owned(),
        }
        .to_string(),
    ]
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn accepted_input_recovery_retains_exact_meaning_and_applied_cursor() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let source = case.claimed_source().await;
        let plan = proposed(&fixture, &source, &task.task_id, TaskInputAction::Steer).await;
        let plan = garde::Unvalidated::new(plan)
            .validate()
            .expect("paid input interpretation");
        let bound = fixture
            .tasks
            .interpret(case.lease(), &plan)
            .await
            .expect("accepted steer");
        let item = bound.items.first().expect("one accepted input");
        let before = fixture
            .tasks
            .inputs(case.lease(), &query(case, None, None))
            .await
            .expect("recovered accepted meaning");
        assert_recovered(&before, &plan, &item.item_id, false);
        let receipt = apply_receipt(case, &plan, &item.item_id, task);
        let accepted = fixture
            .tasks
            .accept_input(case.lease(), &receipt)
            .await
            .expect("incorporated once");
        assert!(!accepted.replayed);
        assert_eq!(accepted.covered_input_sequence, Some(1));
        let restored = fixture
            .tasks
            .inputs(case.lease(), &query(case, None, Some(0)))
            .await
            .expect("recovery before applied boundary");
        assert_recovered(&restored, &plan, &item.item_id, true);
        let after = fixture
            .tasks
            .inputs(case.lease(), &query(case, None, Some(1)))
            .await
            .expect("current history boundary");
        assert_eq!(after.accepted_inputs.len(), 0);
        let replay = fixture
            .tasks
            .accept_input(case.lease(), &receipt)
            .await
            .expect("safe-boundary receipt replay");
        assert!(replay.replayed);
        assert_eq!(replay.covered_input_sequence, Some(1));
        assert_eq!(fixture.counts().await, (1, 1, 1));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

fn assert_recovered(
    page: &af_wire::task::TaskInputsResponse<'_>,
    plan: &af_wire::task::TaskBindingsInput,
    item: &str,
    applied: bool,
) {
    assert_eq!(page.accepted_inputs.len(), 1);
    let input = page.accepted_inputs.first().expect("one retained input");
    assert_eq!(input.item_id, item);
    assert_eq!(input.source_admission_id, plan.source_admission_id);
    assert_eq!(input.plan_digest, plan.plan_digest);
    assert_eq!(input.sequence, 1);
    assert_eq!(input.task.generation, 1);
    assert_eq!(input.action, TaskInputAction::Steer);
    assert_eq!(input.original_message, GOALS.join("; "));
    assert_eq!(input.accepted_text, "Repair");
    assert_eq!(input.interpretation, "Preserve title decoding behavior");
    assert_eq!(input.applied, applied);
    assert!(input.response_text.is_none());
    let source = page.inputs.first().expect("source still owes its answer");
    let meaning = source
        .accepted
        .as_ref()
        .expect("immutable source interpretation");
    assert_eq!(meaning.plan_digest, plan.plan_digest);
}

fn apply_receipt(
    case: &Case,
    plan: &af_wire::task::TaskBindingsInput,
    item: &str,
    task: af_wire::task::DecisionTaskVersion,
) -> garde::Valid<TaskInputReceiptInput> {
    garde::Unvalidated::new(TaskInputReceiptInput {
        fencing_token: case.lease().fencing_token,
        operation_id: afd_db::test_util::mint_id().into(),
        source_admission_id: plan.source_admission_id.clone(),
        plan_digest: plan.plan_digest.clone(),
        item_id: item.to_owned(),
        task: Some(task),
        mode: TaskInputMode::ApplyHere,
        response_text: None,
    })
    .validate()
    .expect("safe-boundary apply receipt")
}
