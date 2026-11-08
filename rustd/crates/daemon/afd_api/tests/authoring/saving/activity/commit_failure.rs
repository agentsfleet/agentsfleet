//! A deferred refusal distinguishes post-commit publication from early publication.

use super::{fault::Refusal, fixture::*, streams::Reader};
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn failed_access_commit_emits_nothing_and_retry_announces_committed_history() {
    let saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    let mut reader = Reader::open(&saved, &saved.fleet).await;
    let fault = Refusal::at_commit(
        saved.case.base.owner_url(),
        &saved.case.base.workspace,
        "access_saved",
    )
    .await;
    assert_eq!(
        saved.access(Method::POST, "fixture-access").await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    fault.assert_fired_once().await;
    reader.quiet().await;
    assert_receipts(&saved.receipts().await, &[saved_receipt()]);
    drop(fault);
    assert_eq!(
        saved.access(Method::POST, "fixture-access").await.status(),
        StatusCode::CREATED
    );
    reader.changed().await;
    reader.quiet().await;
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), access_receipt()],
    );
    saved.case.cleanup().await;
}

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn failed_memory_commit_emits_nothing_and_retry_announces_committed_grants() {
    let saved = Saved::create(json!({})).await;
    let before = saved.grants().await;
    let mut reader = Reader::open(&saved, &saved.fleet).await;
    let fault = Refusal::at_commit(
        saved.case.base.owner_url(),
        &saved.case.base.workspace,
        "memory_sharing_changed",
    )
    .await;
    assert_eq!(
        saved.memory(true, true).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    fault.assert_fired_once().await;
    reader.quiet().await;
    assert_eq!(saved.grants().await, before);
    assert_receipts(&saved.receipts().await, &[saved_receipt(), ready_receipt()]);
    drop(fault);
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    reader.changed().await;
    reader.quiet().await;
    assert_eq!(saved.grants().await, json!({"read":true,"publish":true}));
    assert_receipts(
        &saved.receipts().await,
        &[saved_receipt(), ready_receipt(), memory_receipt(true, true)],
    );
    saved.case.cleanup().await;
}
