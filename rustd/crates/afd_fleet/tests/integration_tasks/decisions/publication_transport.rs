//! Exact-head mutation through the real native allocation, issue and receipt path.
use super::{
    super::fixture::{Case, exercise},
    fixture::Decisions,
    github_authority,
    github_server::Server,
    publication_refusal::{endpoint, prepared},
};
use afd_fleet::task::TaskLifecycle as _;
use afd_wire::task::TaskEffectState;
use base64::{Engine as _, prelude::BASE64_STANDARD};
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn exact_tested_tree_issues_one_mutation_and_retains_attributable_receipt() {
    exercise(publish).await;
}

async fn publish(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let server = Server::start(vec![]).await;
    let tasks = github_authority::publisher(&fixture, server.client.clone()).await;
    let input = garde::Unvalidated::new(prepared(case, task).await)
        .validate()
        .expect("bounded publication");
    script(&server);
    let (receipt, replayed) = tasks
        .publish_effect(case.lease(), &input)
        .await
        .expect("native publication");
    assert!(!replayed);
    assert_eq!(receipt.state, TaskEffectState::Confirmed);
    assert_eq!(receipt.commit, Some("3".repeat(40)));
    let (again, replayed) = tasks
        .publish_effect(case.lease(), &input)
        .await
        .expect("same effect replay");
    assert!(replayed);
    assert_eq!(again, receipt);
    counts(case, &input.task.task_id).await;
    server.finish().await;
    fixture.model.finish().await.expect("fixture cleanup");
}

async fn counts(case: &Case, task: &str) {
    let row: (i32, i64, String, i64) = sqlx::query_as("SELECT used_publications,confirmed_effect_watermark,target->>'head',(SELECT count(*) FROM fleet.task_effect_recovery r JOIN fleet.task_effects e ON e.id=r.task_effect_id WHERE e.task_id=t.id) FROM fleet.tasks t WHERE id=$1::uuid")
        .bind(task).fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("observer")).await.expect("native receipt counters");
    assert_eq!(row, (1, 1, "3".repeat(40), 0));
}

fn push(server: &Server, method: &str, path: &str, response: Value) {
    server.push(json!({"fixture_request":{"method":method,"path":format!("/repos/example/linkwarden/{path}")},"response":response}));
}

fn workflow(server: &Server, head: &str) {
    push(
        server,
        "GET",
        &format!("contents/.github/workflows?ref={head}"),
        json!([{"path":".github/workflows/test.yml","sha":"workflow-blob","type":"file"}]),
    );
    push(
        server,
        "GET",
        "git/blobs/workflow-blob",
        json!({"encoding":"base64","content":BASE64_STANDARD.encode(github_authority::WORKFLOW)}),
    );
}

fn script(server: &Server) {
    push(
        server,
        "GET",
        "pulls/101",
        json!({"number":101,"state":"open","merged":false,"head":endpoint("repair"),"base":endpoint("main")}),
    );
    push(
        server,
        "GET",
        "branches/repair",
        json!({"protected":false,"commit":{"sha":"0".repeat(40)}}),
    );
    workflow(server, &"0".repeat(40));
    push(
        server,
        "GET",
        "branches/main",
        json!({"commit":{"sha":"4".repeat(40)}}),
    );
    workflow(server, &"4".repeat(40));
    push(
        server,
        "GET",
        &format!("git/commits/{}", "0".repeat(40)),
        json!({"sha":"0".repeat(40),"tree":{"sha":"1".repeat(40)}}),
    );
    push(
        server,
        "GET",
        &format!("git/trees/{}", "1".repeat(40)),
        json!({"tree":[],"truncated":false}),
    );
    objects(server);
    server.push(json!({"fixture_request":{"method":"POST","path":"/graphql","publication":true},
        "response":{"data":{"createCommitOnBranch":{"commit":{"oid":"3".repeat(40),"tree":{"oid":"2".repeat(40)},"parents":{"nodes":[{"oid":"0".repeat(40)}]}}}}}}));
}

fn objects(server: &Server) {
    server.push(
        json!({"fixture_request":{"method":"POST","path":"/repos/example/linkwarden/git/blobs",
        "body":{"encoding":"base64","content":"Zml4"}},"response":{"sha":"5".repeat(40)}}),
    );
    server.push(json!({"fixture_request":{"method":"POST","path":"/repos/example/linkwarden/git/trees",
        "body":{"base_tree":"1".repeat(40),"tree":[{"path":"README.md","mode":"100644","type":"blob","sha":"5".repeat(40)}]}},"response":{"sha":"2".repeat(40)}}));
}
