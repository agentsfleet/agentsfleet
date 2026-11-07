//! Native history reconciliation through pinned HTTPS and actual ownership gates.
use super::{
    super::fixture::{Case, exercise, id},
    effect_readback::seed,
    fixture::Decisions,
    github_authority,
    github_server::Server,
};
use afd_fleet::task::{TaskLifecycle as _, TaskRefusal};
use afd_wire::task::TaskEffectState;
use serde_json::{Value, json};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn remote_history_requires_exact_receipt_tree_and_parent_before_confirmation() {
    exercise(history).await;
}

async fn history(case: &Case) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let server = Server::start(vec![]).await;
    let tasks = github_authority::tasks(&fixture, &server).await;
    let effect = seed(case, &task.task_id, 1, "unknown").await;
    server.push(json!({"errors":[{"message":"fixture unavailable"}]}));
    server.push(json!({"data":{"repository":{"databaseId":102}}}));
    server.push(response(&[]));
    server.push(response(&[commit(&effect, "2", "0")]));
    server.push(response(&[commit(&effect, "1", "2")]));
    server.push(response(&[commit(&effect, "1", "0")]));
    observe(&tasks, case, &effect).await;
    server.finish().await;
    fixture.model.finish().await.expect("fixture cleanup");
}

async fn observe(tasks: &afd_fleet::task::Tasks, case: &Case, effect: &str) {
    for _ in 0..2 {
        assert_eq!(
            tasks
                .effect(case.lease(), &id(effect))
                .await
                .expect_err("remote failure or mismatched repository leaves effect uncertain")
                .code(),
            TaskRefusal::EffectUncertain.code()
        );
    }
    for _ in 0..3 {
        let receipt = tasks
            .effect(case.lease(), &id(effect))
            .await
            .expect("authoritative unresolved history");
        assert_eq!(receipt.state, TaskEffectState::Unknown);
        assert!(receipt.commit.is_none());
    }
    let receipt = tasks
        .effect(case.lease(), &id(effect))
        .await
        .expect("exact attributable history");
    assert_eq!(receipt.state, TaskEffectState::Confirmed);
    assert_eq!(
        receipt.commit.as_deref(),
        Some("3333333333333333333333333333333333333333")
    );
    let replay = tasks
        .effect(case.lease(), &id(effect))
        .await
        .expect("durable confirmed read without network");
    assert_eq!(receipt, replay);
}

fn response(nodes: &[Value]) -> Value {
    json!({"data":{"repository":{"databaseId":101,"ref":{"target":{"history":{"nodes":nodes}}}}}})
}

fn commit(effect: &str, tree: &str, parent: &str) -> Value {
    json!({"oid":"3".repeat(40),"message":format!("Fixture repair\n\nagentsfleet-effect:{effect}"),
        "tree":{"oid":tree.repeat(40)},"parents":{"nodes":[{"oid":parent.repeat(40)}]}})
}
