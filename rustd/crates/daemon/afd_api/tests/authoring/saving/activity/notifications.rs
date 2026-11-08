//! A lost notification leaves committed history available to reconnecting readers.

use super::fixture::*;
use afd_auth::Scope;
use afd_dragonfly::{
    Dragonfly,
    config::{DragonflyConfig, DragonflyRole},
};
use http::{Method, StatusCode};
use serde_json::json;
use std::time::Duration;

#[tokio::test]
#[ignore = "needs isolated local Postgres and Dragonfly"]
async fn failed_notifications_keep_access_and_memory_committed_and_reloadable() {
    let mut saved = Saved::create(json!({"credentials":[SERVICE]})).await;
    let config =
        DragonflyConfig::from_url(DragonflyRole::Default, "redis://127.0.0.1:1".to_owned())
            .with_request_timeout(Duration::from_millis(200));
    let queue = Dragonfly::unreachable(&config).expect("lazy unavailable queue");
    saved.case.router = saved
        .case
        .base
        .services(&Scope::ALL)
        .with_fleet_queue(saved.case.base.db.clone(), queue)
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
