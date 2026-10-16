use super::{Case, TOKEN, check, json};

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn refresh_connections_need_no_routing_row_and_do_not_imply_fleet_approval() {
    let case = Case::create(json!({"credentials":["jira"]})).await;
    // A declared credential the workspace lacks is named as a pending check
    // rather than refusing Save.
    check(&case.read().await, "credential", "jira", "missing");
    case.store("jira", json!({"integration":"jira","refresh_token":TOKEN}))
        .await;
    let review = case.read().await;
    check(&review, "credential", "jira", "ready");
    check(&review, "connection", "jira", "ready");
    check(&review, "grant", "jira", "missing");
    case.cleanup().await;
}
