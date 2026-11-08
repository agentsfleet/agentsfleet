//! Accounted message decisions authorize one immutable native interpretation.

use af_core::{error_code, etag::sha256_hex};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    task::{
        DecisionAssessment, DecisionReceipt, TaskBindingItem, TaskBindingsInput, TaskInputAction,
        TaskProposedItem, TaskRequestedTarget,
    },
};
use afd_fleet::task::TaskLifecycle as _;

use super::super::fixture::{Case, exercise};
use super::{
    fixture::{Decisions, request},
    reply::known_result,
};

fn digest(input: &mut TaskBindingsInput) {
    let digest = sha256_hex(
        &serde_json::to_vec(&input.interpretation()).expect("stable interpretation encoding"),
    );
    input.plan_digest = digest;
}

fn valid(input: TaskBindingsInput) -> garde::Valid<TaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded interpretation")
}

pub(super) async fn proposed(
    fixture: &Decisions<'_>,
    action: TaskInputAction,
) -> TaskBindingsInput {
    let (_, body) = fixture
        .reserve(&fixture.reservation(DecisionAssessment::Message, None))
        .await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("complete fixture answer");
    answers.insert(
        "multiple_requests".to_owned(),
        DecisionAnswer::Noul { noul: 0.0 },
    );
    choose(answers, "intent", action.as_str());
    if action == TaskInputAction::NewWork {
        choose(answers, "association", "new_work");
    }
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    input(fixture, request.call_id, action)
}

fn input(fixture: &Decisions<'_>, call: String, action: TaskInputAction) -> TaskBindingsInput {
    let mut input = TaskBindingsInput {
        fencing_token: fixture.case.lease().fencing_token,
        source_admission_id: fixture.case.held.event_id.clone().into(),
        accepted_revision_id: fixture.revision.to_string(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: call,
        items: vec![TaskBindingItem {
            decision_call_id: None,
            proposed: TaskProposedItem {
                source_start: 0,
                source_end: 6,
                action,
                task_id: None,
                target: (action == TaskInputAction::NewWork)
                    .then_some(TaskRequestedTarget::ApiOnly),
                title: "Explain repair request".to_owned(),
                interpretation: "Discuss the submitted repair request before accepting work"
                    .to_owned(),
            },
        }],
    };
    digest(&mut input);
    input
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_conversation_interpretation_replays_without_inventing_a_task() {
    exercise(conversation).await;
}

async fn conversation(case: &Case) {
    let fixture = Decisions::create(case).await;
    let input = valid(proposed(&fixture, TaskInputAction::Conversation).await);
    let accepted = fixture
        .tasks
        .interpret(case.lease(), &input)
        .await
        .expect("paid interpretation");
    assert!(!accepted.replayed);
    assert_eq!(accepted.items.len(), 1);
    let item = accepted.items.first().expect("one conversational item");
    assert!(item.task_id.is_none());
    assert_eq!(item.action, TaskInputAction::Conversation);
    assert!(accepted.current_task.is_none());
    assert_eq!(case.counts().await, (0, 1, 1));
    let replay = fixture
        .tasks
        .interpret(case.lease(), &input)
        .await
        .expect("same interpretation replay");
    assert!(replay.replayed);
    assert_eq!(replay.items, accepted.items);
    assert_eq!(case.counts().await, (0, 1, 1));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.task_interpretations WHERE admission_id=$1")
            .bind(&case.held.event_id)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("receipt observer"),
            )
            .await
            .expect("one immutable interpretation");
    assert_eq!(count, 1);
    fixture.model.finish().await.expect("model cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn interpretation_refuses_unpaid_mismatched_and_out_of_bounds_plans_atomically() {
    exercise(refusals).await;
}

async fn refusals(case: &Case) {
    let fixture = Decisions::create(case).await;
    let original = proposed(&fixture, TaskInputAction::Conversation).await;
    let mut bad_digest = original.clone();
    bad_digest.plan_digest = "f".repeat(64);
    let mut unpaid = original.clone();
    unpaid.message_decision_call_id = afd_db::test_util::mint_id().into();
    let mut out_of_bounds = original.clone();
    out_of_bounds
        .items
        .first_mut()
        .expect("one item")
        .proposed
        .source_end = 16384;
    digest(&mut out_of_bounds);
    let mut different_intent = original.clone();
    different_intent
        .items
        .first_mut()
        .expect("one item")
        .proposed
        .action = TaskInputAction::Clarify;
    digest(&mut different_intent);
    for (input, code) in [
        (bad_digest, error_code::TASK_REPLAY_CONFLICT),
        (unpaid, error_code::TASK_NOT_FOUND),
        (out_of_bounds, error_code::TASK_INPUT_AMBIGUOUS),
        (different_intent, error_code::TASK_INPUT_AMBIGUOUS),
    ] {
        let error = fixture
            .tasks
            .interpret(case.lease(), &valid(input))
            .await
            .expect_err("invalid interpretation");
        assert_eq!(error.code(), code);
        assert_eq!(case.counts().await, (0, 0, 1));
    }
    fixture.model.finish().await.expect("model cleanup");
}

pub(super) fn choose(
    answers: &mut std::collections::BTreeMap<String, DecisionAnswer>,
    name: &str,
    selected: &str,
) {
    let (choice, confidence, probabilities) = match answers.get_mut(name) {
        Some(DecisionAnswer::Choice {
            choice,
            confidence,
            probabilities,
        }) => Some((choice, confidence, probabilities)),
        _ => None,
    }
    .expect("configured choice answer");
    selected.clone_into(choice);
    *confidence = Some(1.0);
    for (option, probability) in probabilities {
        *probability = if option == selected { 1.0 } else { 0.0 };
    }
}
