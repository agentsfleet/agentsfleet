use super::super::mint;
use super::{Case, SLACK_SCOPES, TOKEN, check, json, mention};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_foreign_routing_install_never_satisfies_the_reviewed_workspace() {
    let case = Case::create(mention("slack")).await;
    let foreign = Case::create(mention("slack")).await;
    let team = format!("T{}", mint());
    case.store(
        "slack",
        json!({"integration":"slack","team_id":team,"bot_token":TOKEN}),
    )
    .await;
    foreign.install(&team, SLACK_SCOPES).await;
    check(&case.read().await, "connection", "slack", "missing");
    foreign.cleanup().await;
    let id = case.install(&team, SLACK_SCOPES).await;
    let before = case.read().await;
    check(&before, "connection", "slack", "ready");
    case.remove_install(&id).await;
    check(&case.read().await, "connection", "slack", "missing");
    case.install(&team, SLACK_SCOPES).await;
    let replacement = case.read().await;
    check(&replacement, "connection", "slack", "ready");
    assert_ne!(before["review_digest"], replacement["review_digest"]);
    case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_shared_mention_and_webhook_name_must_satisfy_both_uses() {
    let case = Case::create(json!({"triggers":[
        {"type":"mention","source":"slack","channels":["C012345678"]},
        {"type":"webhook","source":"slack"}]}))
    .await;
    let team = format!("T{}", mint());
    case.store(
        "slack",
        json!({"integration":"slack","team_id":team,"bot_token":TOKEN}),
    )
    .await;
    case.install(&team, SLACK_SCOPES).await;
    check(&case.read().await, "credential", "slack", "blocked");
    case.cleanup().await;
}
