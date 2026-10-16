use super::super::mint;
use super::{Case, SLACK_SCOPES, TOKEN, check, json, mention};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn mentions_refuse_generic_objects_and_unknown_providers() {
    for source in ["slack", "unknown-mention"] {
        let case = Case::create(mention(source)).await;
        check(&case.read().await, "credential", source, "missing");
        case.store(source, json!({"api_key":TOKEN})).await;
        check(&case.read().await, "credential", source, "blocked");
        case.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn malformed_slack_handles_cannot_become_ready() {
    for body in [
        json!({"integration":"slack","bot_token":TOKEN}),
        json!({"integration":"slack","team_id":42,"bot_token":TOKEN}),
        json!({"integration":"slack","team_id":"","bot_token":TOKEN}),
        json!({"integration":"slack","team_id":"T123"}),
        json!({"integration":"slack","team_id":"T123","bot_token":42}),
        json!({"integration":"slack","team_id":"T123","bot_token":""}),
    ] {
        let case = Case::create(mention("slack")).await;
        case.store("slack", body).await;
        check(&case.read().await, "credential", "slack", "blocked");
        case.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn slack_requires_its_routing_install_and_actual_scopes() {
    let case = Case::create(mention("slack")).await;
    let team = format!("T{}", mint());
    case.store(
        "slack",
        json!({"integration":"slack","team_id":team,"bot_token":TOKEN}),
    )
    .await;
    check(&case.read().await, "credential", "slack", "ready");
    check(&case.read().await, "connection", "slack", "missing");
    let id = case.install(&team, &["chat:write"]).await;
    check(&case.read().await, "connection", "slack", "blocked");
    case.scopes(&id, SLACK_SCOPES).await;
    let ready = case.read().await;
    check(&ready, "connection", "slack", "ready");
    case.scopes(&id, &[]).await;
    let revoked = case.read().await;
    check(&revoked, "connection", "slack", "blocked");
    assert_ne!(ready["review_digest"], revoked["review_digest"]);
    case.cleanup().await;
}
