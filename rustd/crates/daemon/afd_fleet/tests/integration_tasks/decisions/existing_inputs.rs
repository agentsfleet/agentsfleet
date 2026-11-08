//! Paid association keeps existing-task input on its original execution.
use af_core::{error_code, etag::sha256_hex, id::Uuid7};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    task::{
        DecisionAssessment, DecisionReceipt, TaskBindingItem, TaskBindingsInput, TaskInputAction,
        TaskProposedItem,
    },
};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::json;

use super::{
    super::fixture::{Case, exercise},
    fixture::{Decisions, request},
    interpretation::choose,
    reply::known_result,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_steer_and_refresh_append_once_to_the_running_task() {
    for action in [TaskInputAction::Steer, TaskInputAction::Refresh] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let source = case.claimed_source().await;
            let input = proposed(&fixture, &source, &task.task_id, action).await;
            let accepted = fixture
                .tasks
                .interpret(case.lease(), &valid(input.clone()))
                .await
                .expect("paid existing association");
            assert!(!accepted.replayed);
            let item = accepted.items.first().expect("one associated item");
            assert_eq!(accepted.items.len(), 1);
            assert_eq!(item.task_id.as_deref(), Some(task.task_id.as_str()));
            assert_eq!(item.action, action);
            assert!(item.applies_here);
            assert_eq!(item.input_sequence, Some(1));
            assert_eq!(item.task_generation, Some(1));
            assert_input(case, &task.task_id, action).await;
            let replay = fixture
                .tasks
                .interpret(case.lease(), &valid(input))
                .await
                .expect("accepted association replay");
            assert!(replay.replayed);
            assert_eq!(replay.items, accepted.items);
            assert_input(case, &task.task_id, action).await;
            assert_eq!(fixture.counts().await, (1, 1, 1));
            fixture.model.finish().await.expect("model fixture cleanup");
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_existing_input_refuses_missing_and_foreign_task_without_partial_writes() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let source = case.claimed_source().await;
        let input = proposed(&fixture, &source, &task.task_id, TaskInputAction::Steer).await;
        let before = case.counts().await;
        for (target, code) in [
            (None, error_code::TASK_INVALID_TARGET),
            (
                Some(afd_db::test_util::mint_id()),
                error_code::TASK_NOT_FOUND,
            ),
        ] {
            let mut wrong = input.clone();
            wrong.items.first_mut().expect("one item").proposed.task_id = target;
            digest(&mut wrong);
            let error = fixture
                .tasks
                .interpret(case.lease(), &valid(wrong))
                .await
                .expect_err("unowned association refused");
            assert_eq!(error.code(), code);
            assert_eq!(case.counts().await, before);
            let count: i64 =
                sqlx::query_scalar("SELECT count(*) FROM fleet.task_inputs WHERE task_id=$1::uuid")
                    .bind(&task.task_id)
                    .fetch_one(
                        &mut *case
                            .held
                            .fixtures
                            .database
                            .acquire()
                            .await
                            .expect("input observer"),
                    )
                    .await
                    .expect("unchanged inputs");
            assert_eq!(count, 0);
        }
        assert_eq!(fixture.counts().await, (1, 1, 1));
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

pub(super) async fn proposed(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    task: &str,
    action: TaskInputAction,
) -> TaskBindingsInput {
    let reservation = fixture.reservation(
        DecisionAssessment::Message,
        Some(af_wire::task::DecisionTaskVersion {
            task_id: task.to_owned(),
            generation: 1,
        }),
    );
    let mut reservation = (*reservation).clone();
    reservation.source_admission_id = source.to_string();
    let reservation = garde::Unvalidated::new(reservation)
        .validate()
        .expect("claimed message reservation");
    let (_, body) = fixture.reserve(&reservation).await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("complete decision answers");
    answers.insert(
        "multiple_requests".to_owned(),
        DecisionAnswer::Noul { noul: 0.0 },
    );
    choose(answers, "intent", action.as_str());
    choose(answers, "association", "t0");
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    binding(fixture, source, task, action, request.call_id)
}

fn binding(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    task: &str,
    action: TaskInputAction,
    call: String,
) -> TaskBindingsInput {
    let mut input = TaskBindingsInput {
        fencing_token: fixture.case.lease().fencing_token,
        source_admission_id: source.to_string(),
        accepted_revision_id: fixture.revision.to_string(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: call,
        items: vec![TaskBindingItem {
            decision_call_id: None,
            proposed: TaskProposedItem {
                source_start: 0,
                source_end: 6,
                action,
                task_id: Some(task.to_owned()),
                target: None,
                title: "Repair title decoding".to_owned(),
                interpretation: "Preserve title decoding behavior".to_owned(),
            },
        }],
    };
    digest(&mut input);
    input
}

fn digest(input: &mut TaskBindingsInput) {
    let digest =
        sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("complete interpretation"));
    input.plan_digest = digest;
}

fn valid(input: TaskBindingsInput) -> garde::Valid<TaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded interpretation")
}

async fn assert_input(case: &Case, task: &str, action: TaskInputAction) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("accepted input observer");
    let rows: Vec<(i64, String, String, i64)> = sqlx::query_as("SELECT sequence,accepted_text,interpretation,generation FROM fleet.task_inputs WHERE task_id=$1::uuid")
        .bind(task).fetch_all(&mut *connection).await.expect("accepted input custody");
    assert_eq!(
        rows,
        [(
            1,
            "Repair".to_owned(),
            "Preserve title decoding behavior".to_owned(),
            1
        )]
    );
    let state: (String, i64, i64, i64) = sqlx::query_as("SELECT status,generation,input_sequence,(SELECT count(*) FROM fleet.admissions WHERE fleet_id=$2::uuid) FROM fleet.tasks WHERE id=$1::uuid")
        .bind(task).bind(case.scope.fleet_id.as_str()).fetch_one(&mut *connection).await.expect("same execution");
    assert_eq!(state, ("running".to_owned(), 1, 1, 2));
    let saved: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM fleet.task_items i WHERE task_id=$1::uuid AND action=$2",
    )
    .bind(task)
    .bind(action.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("original accepted action");
    assert_eq!(saved.get("action"), Some(&json!(action.as_str())));
}
