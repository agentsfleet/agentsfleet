//! Public interpretation accepts only the human's verified existing pull request.
use super::{
    super::fixture::{Case, exercise, id},
    checkpoint_server,
    fixture::Decisions,
    github_authority, github_server,
};
use af_core::id::Uuid7;
use af_core::{error_code, etag::sha256_hex};
use af_wire::{
    decision::NoulCriteria,
    durable_task::{
        DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskInputAction,
        DurableTaskRequestedTarget, DurableTaskStatement,
    },
};
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use serde_json::Value;
mod refusals;
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
async fn protected_pull_request_branch_refuses_interpretation_without_creating_durable_tasks() {
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
    let result = tasks.interpret(case.run(), &valid(input.clone())).await;
    if protected {
        assert_eq!(
            result
                .expect_err("protected branch cannot become writable work")
                .code(),
            error_code::DURABLE_TASK_INVALID_TARGET
        );
        assert_eq!(case.counts().await, before);
    } else {
        accepted(case, &tasks, input, result.expect("verified human target")).await;
    }
    assert_eq!(
        fixture.counts().await,
        (0, 0, 0),
        "a binding makes no decision call"
    );
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

/// The person's message naming the pull request, folded into the run, and
/// the one-item binding a run's task tool sends for it.
async fn proposed(fixture: &Decisions<'_>) -> DurableTaskBindingsInput {
    let case = fixture.case;
    let source = case
        .source_message(
            afd_turns::Producer::Steer,
            &afd_turns::view::steer_actor("native_durable_task_owner"),
            MESSAGE,
        )
        .await;
    case.assign(&source).await;
    binding(case, source.to_string())
}

fn binding(case: &Case, source: String) -> DurableTaskBindingsInput {
    let mut input = DurableTaskBindingsInput {
        fencing_token: case.run().fencing_token,
        source_turn_id: source,
        plan_digest: "0".repeat(64),
        items: vec![DurableTaskBindingItem::NewWork {
            source_start: 0,
            source_end: i32::try_from(MESSAGE.len()).expect("bounded source"),
            title: "Repair assigned pull request".to_owned(),
            target: DurableTaskRequestedTarget::ExistingPr {
                repository: "example/linkwarden".to_owned(),
                number: 101,
            },
            statements: vec![DurableTaskStatement {
                id: "checks_green".to_owned(),
                question: "Do the pull request's required checks pass at its head?".to_owned(),
                criteria: NoulCriteria {
                    yes: "Every required check passed at the current head.".to_owned(),
                    no: "A required check failed, is pending, or covers older code.".to_owned(),
                },
            }],
        }],
    };
    let digest =
        sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("exact interpretation"));
    input.plan_digest = digest;
    input
}

fn valid(input: DurableTaskBindingsInput) -> garde::Valid<DurableTaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded target interpretation")
}

async fn assert_target(case: &Case, task: &Uuid7) {
    let target: Value = sqlx::query_scalar("SELECT target FROM fleet.durable_tasks WHERE id=$1")
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
    tasks: &afd_fleet::durable_task::DurableTasks,
    input: DurableTaskBindingsInput,
    accepted: af_wire::durable_task::DurableTaskBindingsResponse<'_>,
) {
    assert!(!accepted.replayed);
    assert_eq!(accepted.items.len(), 1);
    let item = accepted.items.first().expect("one accepted repair");
    assert!(!item.applies_here, "the calling run stays an ordinary run");
    assert_eq!(item.action, DurableTaskInputAction::NewWork);
    let task = id(item.durable_task_id.as_ref().expect("native target owner"));
    assert_target(case, &task).await;
    let replay = tasks
        .interpret(case.run(), &valid(input))
        .await
        .expect("preflight result replay");
    assert!(replay.replayed);
    assert_eq!(replay.items, accepted.items);
    assert_eq!(
        case.counts().await,
        (1, 1, 3),
        "the task and its staged continuation"
    );
}
