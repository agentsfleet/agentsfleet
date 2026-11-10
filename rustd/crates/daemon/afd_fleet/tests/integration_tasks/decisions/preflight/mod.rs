//! Public interpretation accepts only the human's verified existing pull request.
use super::{
    super::fixture::{Case, exercise, id},
    checkpoint_server,
    fixture::{Decisions, request},
    github_authority, github_server,
    interpretation::choose,
    reply::known_result,
};
use af_core::id::Uuid7;
use af_core::{error_code, etag::sha256_hex};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    task::{
        DecisionAssessment, DecisionReceipt, TaskBindingItem, TaskBindingsInput, TaskInputAction,
        TaskProposedItem, TaskRequestedTarget,
    },
};
use afd_fleet::task::TaskLifecycle as _;
use serde_json::Value;
mod remote;
mod storage;
const MESSAGE: &str = "Repair https://github.com/example/linkwarden/pull/101";

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn explicit_pull_request_interpretation_verifies_storage_head_and_both_workflow_trees() {
    exercise(async |case| scenario(case, false).await).await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn protected_pull_request_branch_refuses_interpretation_without_creating_tasks() {
    exercise(async |case| scenario(case, true).await).await;
}

async fn scenario(case: &Case, protected: bool) {
    let mut fixture = Decisions::create(case).await;
    let objects = checkpoint_server::Server::start().await;
    storage::configure(&mut fixture, &objects).await;
    let remote = github_server::Server::start(remote::responses(protected)).await;
    let tasks = github_authority::publisher(&fixture, &remote).await;
    let input = proposed(&fixture).await;
    let before = case.counts().await;
    let result = tasks.interpret(case.lease(), &valid(input.clone())).await;
    if protected {
        assert_eq!(
            result
                .expect_err("protected branch cannot become writable work")
                .code(),
            error_code::TASK_INVALID_TARGET
        );
        assert_eq!(case.counts().await, before);
    } else {
        accepted(case, &tasks, input, result.expect("verified human target")).await;
    }
    assert_eq!(fixture.counts().await, (1, 1, 1));
    assert_eq!(
        objects
            .objects
            .lock()
            .expect("probe cleanup observer")
            .len(),
        0
    );
    remote.finish().await;
    objects.finish().await;
    fixture.model.finish().await.expect("model cleanup");
}

async fn proposed(fixture: &Decisions<'_>) -> TaskBindingsInput {
    let case = fixture.case;
    let source = case
        .source_message(
            afd_admission::Producer::Steer,
            &afd_events::steer_actor("native_task_owner"),
            MESSAGE,
        )
        .await;
    let reservation = fixture.reservation(DecisionAssessment::Message, None);
    let mut reservation = (*reservation).clone();
    reservation.source_admission_id = source.to_string();
    let reservation = garde::Unvalidated::new(reservation)
        .validate()
        .expect("explicit human request");
    let (_, body) = fixture.reserve(&reservation).await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("complete model receipt");
    answers.insert(
        "multiple_requests".to_owned(),
        DecisionAnswer::Noul { noul: 0.0 },
    );
    choose(answers, "intent", "new_work");
    choose(answers, "association", "new_work");
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    let revision = revision(case).await;
    binding(case, source.to_string(), revision.into(), request.call_id)
}

fn binding(case: &Case, source: String, revision: String, call: String) -> TaskBindingsInput {
    let mut input = TaskBindingsInput {
        fencing_token: case.lease().fencing_token,
        source_admission_id: source,
        accepted_revision_id: revision,
        plan_digest: "0".repeat(64),
        message_decision_call_id: call,
        items: vec![TaskBindingItem {
            decision_call_id: None,
            proposed: TaskProposedItem {
                source_start: 0,
                source_end: i32::try_from(MESSAGE.len()).expect("bounded source"),
                action: TaskInputAction::NewWork,
                task_id: None,
                target: Some(TaskRequestedTarget::ExistingPr {
                    repository: "example/linkwarden".to_owned(),
                    number: 101,
                }),
                title: "Repair assigned pull request".to_owned(),
                interpretation: "Repair the explicitly assigned pull request".to_owned(),
            },
        }],
    };
    let digest =
        sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("exact interpretation"));
    input.plan_digest = digest;
    input
}

fn valid(input: TaskBindingsInput) -> garde::Valid<TaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded target interpretation")
}

async fn assert_target(case: &Case, task: &Uuid7) {
    let target: Value = sqlx::query_scalar("SELECT target FROM fleet.tasks WHERE id=$1")
        .bind(task)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("persisted target observer"),
        )
        .await
        .expect("verified target custody");
    let installation = target
        .get("installation_id")
        .and_then(Value::as_u64)
        .expect("real installation identifier");
    assert!(installation > 0);
    assert_eq!(
        target,
        serde_json::json!({"repository":"example/linkwarden","repository_id":101,
        "installation_id":installation,"number":101,"branch":"repair","base":"main","head":"1".repeat(40),"tree":"2".repeat(40),
        "url":"https://github.com/example/linkwarden/pull/101",
        "workflows":[{"id":42,"path":".github/workflows/test.yml","digest":sha256_hex(github_authority::WORKFLOW.as_bytes())}]})
    );
}

async fn accepted(
    case: &Case,
    tasks: &afd_fleet::task::Tasks,
    input: TaskBindingsInput,
    accepted: af_wire::task::TaskBindingsResponse<'_>,
) {
    assert!(!accepted.replayed);
    assert_eq!(accepted.items.len(), 1);
    let item = accepted.items.first().expect("one accepted repair");
    assert!(item.applies_here);
    assert_eq!(item.action, TaskInputAction::NewWork);
    let task = id(item.task_id.as_ref().expect("native target owner"));
    assert_target(case, &task).await;
    let replay = tasks
        .interpret(case.lease(), &valid(input))
        .await
        .expect("preflight result replay");
    assert!(replay.replayed);
    assert_eq!(replay.items, accepted.items);
    assert_eq!(case.counts().await, (1, 1, 2));
}

async fn revision(case: &Case) -> Uuid7 {
    sqlx::query_scalar("SELECT head_revision_id FROM fleet.fleets WHERE id=$1")
        .bind(&case.scope.fleet_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("accepted setup observer"),
        )
        .await
        .expect("setup owning repository policy")
}
