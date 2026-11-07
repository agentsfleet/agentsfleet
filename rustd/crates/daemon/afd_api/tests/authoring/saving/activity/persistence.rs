//! HTTP mutations are visible through new store handles, scoped to their recipients.

use super::super::super::{json_body, send};
use super::fixture::*;
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn receipt_history_reloads_exact_safe_setup_access_and_memory_facts() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    assert_eq!(saved.initial["status"], "installing");
    assert_receipts(&saved.receipts().await, &[saved_receipt()]);
    let value = format!("fixture-{}", super::super::super::mint());
    assert_eq!(
        saved.access(Method::POST, &value).await.status(),
        StatusCode::CREATED
    );
    assert_eq!(saved.activate().await.status(), StatusCode::OK);
    assert_eq!(saved.activate().await.status(), StatusCode::OK);
    assert_eq!(saved.memory(true, false).await.status(), StatusCode::OK);
    assert_eq!(saved.memory(true, false).await.status(), StatusCode::OK);
    assert_receipts(
        &saved.receipts().await,
        &[
            saved_receipt(),
            access_receipt(),
            ready_receipt(),
            memory_receipt(true, false),
        ],
    );
    assert!(
        !saved.get("/messages").await.to_string().contains(&value),
        "no credential value in history"
    );
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn access_receipts_reach_only_relevant_saved_fleets_in_the_same_workspace() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    let dependent = saved
        .neighbor("Support triage", json!({"credentials":[SERVICE]}))
        .await;
    let trigger = saved
        .neighbor(
            "Delivery review",
            json!({"credentials":[],"triggers":[
                {"type":"webhook", "source":"github", "credential_name":SERVICE}
            ]}),
        )
        .await;
    let unrelated = saved
        .neighbor(
            "Pull request review",
            json!({"credentials":[],"triggers":[]}),
        )
        .await;
    let other = Saved::create(json!({"credentials":[SERVICE]})).await;
    assert_eq!(
        saved.access(Method::POST, "fixture-access").await.status(),
        StatusCode::CREATED
    );
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), access_receipt()],
    );
    assert_receipts(&saved.receipts_for(&dependent).await, &[access_receipt()]);
    assert_receipts(&saved.receipts_for(&trigger).await, &[access_receipt()]);
    assert_receipts(&saved.receipts_for(&unrelated).await, &[]);
    assert_receipts(&other.receipts().await, &[saved_receipt()]);
    assert_foreign_history_empty(&saved, &other).await;
    other.case.cleanup().await;
    saved.case.cleanup().await;
}

async fn assert_foreign_history_empty(saved: &Saved, other: &Saved) {
    let path = format!(
        "/v1/workspaces/{}/fleets/{}/messages",
        saved.case.base.workspace, other.fleet
    );
    let refused = send(
        &saved.reloaded_router(),
        Method::GET,
        &path,
        Some(&saved.case.base.token),
        "",
    )
    .await;
    assert_eq!(refused.status(), StatusCode::OK);
    assert_eq!(
        json_body(refused).await["items"],
        json!([]),
        "foreign fleet receipts never enter this workspace's history"
    );
}
