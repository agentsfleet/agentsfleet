//! A lost notification leaves committed history available to reconnecting readers.

use super::fixture::*;
use afd_auth::Scope;
use http::{Method, StatusCode};
use serde_json::json;

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn failed_notifications_keep_access_and_memory_committed_and_reloadable() {
    let mut saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    saved.case.router = saved
        .case
        .base
        .services(&Scope::ALL)
        .with_fleet_queue(
            saved.case.base.db.clone(),
            afd_bus::test_util::unreachable(),
        )
        .router();
    assert_eq!(
        saved.access(Method::POST, "fixture-first").await.status(),
        StatusCode::CREATED
    );
    assert_eq!(
        saved.access(Method::PUT, "fixture-replaced").await.status(),
        StatusCode::OK
    );
    super::rollback::assert_secret(&saved, Some("fixture-replaced")).await;
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    assert_eq!(saved.memory(true, true).await.status(), StatusCode::OK);
    assert_eq!(saved.grants().await, json!({"read":true,"publish":true}));
    assert_receipts(
        &saved.receipts().await,
        &[
            saved_receipt(),
            access_receipt(),
            access_receipt(),
            memory_receipt(true, true),
        ],
    );
    saved.case.cleanup().await;
}
