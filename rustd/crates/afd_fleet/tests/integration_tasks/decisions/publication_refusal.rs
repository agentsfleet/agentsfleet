//! Native allocation and remote preflight refusal, with no branch mutation.
use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
    github_authority,
    github_server::Server,
};
use afd_db::test_util::mint_id;
use afd_fleet::task::{TaskLifecycle as _, Tasks};
use afd_wire::task::{DecisionTaskVersion, TaskEffectInput, TaskEffectState, TaskFileChange};
use serde_json::json;
use sqlx::types::Json;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn publication_allocates_once_and_closed_pr_refuses_before_any_ref_mutation() {
    exercise(refusal).await;
}

async fn refusal(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let server = Server::start(vec![]).await;
    let tasks = github_authority::publisher(&fixture, server.client.clone()).await;
    let input = prepared(case, task).await;
    stale_inputs(&tasks, case, &input).await;
    server.push(
        json!({"fixture_request":{"method":"GET","path":"/repos/example/linkwarden/pulls/101"},
        "response":{"number":101,"state":"closed","merged":false,
            "head":endpoint("repair"),"base":endpoint("main")}}),
    );
    assert_refusal(&tasks, case, input).await;
    server.finish().await;
    fixture.model.finish().await.expect("fixture cleanup");
}

async fn assert_refusal(tasks: &Tasks, case: &Case, input: TaskEffectInput) {
    let valid = garde::Unvalidated::new(input.clone())
        .validate()
        .expect("valid publication");
    assert!(tasks.publish_effect(case.lease(), &valid).await.is_err());
    let (receipt, replayed) = tasks
        .publish_effect(case.lease(), &valid)
        .await
        .expect("durable refused replay");
    assert!(replayed);
    assert_eq!(receipt.state, TaskEffectState::Refused);
    assert!(receipt.commit.is_none());
    assert_eq!(receipt.ordinal, 1);
    conflict(tasks, case, input).await;
    let counts: (i64, i32) = sqlx::query_as("SELECT (SELECT count(*) FROM fleet.task_effects WHERE task_id=t.id), used_publications FROM fleet.tasks t WHERE id=$1::uuid")
        .bind(&valid.task.task_id).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("observer")).await.expect("publication counters");
    assert_eq!(counts, (1, 0));
}

async fn conflict(tasks: &Tasks, case: &Case, mut input: TaskEffectInput) {
    input.tested_tree = "3".repeat(40);
    let valid = garde::Unvalidated::new(input)
        .validate()
        .expect("different valid request");
    assert!(tasks.publish_effect(case.lease(), &valid).await.is_err());
}

async fn stale_inputs(tasks: &Tasks, case: &Case, input: &TaskEffectInput) {
    for mismatch in ["source", "head", "cursor", "generation"] {
        let mut changed = input.clone();
        match mismatch {
            "source" => changed.source_item_id = mint_id(),
            "head" => changed.expected_head = "f".repeat(40),
            "cursor" => changed.covered_input_sequence += 1,
            _ => changed.task.generation += 1,
        }
        let valid = garde::Unvalidated::new(changed)
            .validate()
            .expect("well-shaped stale request");
        assert!(tasks.publish_effect(case.lease(), &valid).await.is_err());
    }
}

pub(super) async fn prepared(case: &Case, task: DecisionTaskVersion) -> TaskEffectInput {
    let workflows = json!([{"id":42,"path":".github/workflows/test.yml","digest":afd_core::etag::sha256_hex(github_authority::WORKFLOW.as_bytes())}]);
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("accepted publication fixture");
    // Represents a preexisting accepted target with reviewed workflow under this revision.
    // The tested behavior is publication; this fixture does not claim target preflight ran.
    sqlx::query("UPDATE fleet.tasks t SET accepted_revision_id=f.head_revision_id,target=jsonb_set(t.target,'{workflows}',$2) FROM fleet.fleets f WHERE t.id=$1::uuid AND f.id=t.fleet_id")
        .bind(&task.task_id).bind(Json(workflows)).execute(&mut *connection).await.expect("preexisting accepted binding");
    let (source, sequence): (String, i64) = sqlx::query_as(
        "SELECT originating_task_item_id::text,input_sequence FROM fleet.tasks WHERE id=$1::uuid",
    )
    .bind(&task.task_id)
    .fetch_one(&mut *connection)
    .await
    .expect("actual accepted source");
    TaskEffectInput {
        operation_id: mint_id(),
        fencing_token: case.lease().fencing_token,
        task,
        source_item_id: source,
        covered_input_sequence: u64::try_from(sequence).expect("input sequence"),
        expected_head: "0".repeat(40),
        tested_tree: "2".repeat(40),
        changes: vec![TaskFileChange {
            path: "README.md".to_owned(),
            contents: Some("Zml4".to_owned()),
        }],
    }
}

pub(super) fn endpoint(branch: &str) -> serde_json::Value {
    json!({"ref":branch,"sha":"0".repeat(40),"repo":{"id":101,"full_name":"example/linkwarden","default_branch":"main"}})
}
