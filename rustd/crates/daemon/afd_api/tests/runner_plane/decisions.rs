//! Decision bodies are validated after runner authentication and before stores.
#![expect(clippy::expect_used, reason = "test fixture preconditions fail loudly")]

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};
use serde_json::{Value, json};

use super::{FLEET_ID, LEASE_ID, RUNNER_TOKEN, TENANT_KEY, code_of};
use crate::harness::{Fleet, runner_id, send};

#[tokio::test]
async fn decision_routes_preserve_runner_auth_strict_input_and_datastore_failure_classes() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let collection = format!("/v1/runners/me/leases/{LEASE_ID}/tasks/decisions");
    let receipt = format!("{collection}/{FLEET_ID}");
    let reservation = json!({"operation_id": FLEET_ID, "fencing_token": 7,
        "source_admission_id": LEASE_ID, "assessment": {"kind": "message"}, "evidence": {}});
    let accounted = json!({"fencing_token": 7,
        "receipt": {"outcome": "unknown", "cause": {"reason": "timeout"}}});
    for (method, path, body) in [
        (Method::POST, collection.as_str(), reservation),
        (Method::PATCH, receipt.as_str(), accounted),
    ] {
        authentication(&router, &method, path).await;
        strict_input(&router, &method, path, &body).await;
        let admitted = send(&router, method, path, Some(RUNNER_TOKEN), &body.to_string()).await;
        assert_eq!(admitted.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(code_of(admitted).await, "UZ-INTERNAL-001");
    }
    let malformed_path = format!("{collection}/not-a-call");
    let malformed = send(
        &router,
        Method::PATCH,
        &malformed_path,
        Some(RUNNER_TOKEN),
        "{}",
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");
}

async fn authentication(router: &axum::Router, method: &Method, path: &str) {
    for credential in [None, Some(TENANT_KEY)] {
        let refused = send(router, method.clone(), path, credential, "{}").await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(code_of(refused).await, "UZ-RUN-001");
    }
}

async fn strict_input(router: &axum::Router, method: &Method, path: &str, body: &Value) {
    let mut changed = body.clone();
    changed
        .as_object_mut()
        .expect("complete object fixture")
        .insert(
            "profile".to_owned(),
            json!({"endpoint": "https://example.test/untrusted-decision"}),
        );
    let serialized = body.to_string();
    let duplicate = format!(
        "{{\"fencing_token\":1,{}",
        serialized.get(1..).expect("object contents")
    );
    for invalid in ["{}".to_owned(), changed.to_string(), duplicate] {
        let refused = send(router, method.clone(), path, Some(RUNNER_TOKEN), &invalid).await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
        assert_eq!(code_of(refused).await, "UZ-REQ-001");
    }
}
