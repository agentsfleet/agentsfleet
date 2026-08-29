//! The device flow end to end, against a live Redis.
//!
//! The unit half of this surface is `auth_sessions.rs`: what each endpoint
//! refuses before it reaches the queue, which is a question a fake answers. The
//! question here is what a SESSION does across calls — opened, approved,
//! verified twice, cancelled twice, then swept in bulk — and a fake that
//! remembers nothing cannot be asked it.
//!
//! Split from that file rather than sharing it, because the filename is what
//! declares the tier: this one needs Redis and is `#[ignore]`d for it, and the
//! unit lane must be able to run its sibling on a machine with Docker closed.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The collection every session is opened against.
const SESSIONS: &str = "/v1/auth/sessions";

/// The bulk-abort route.
const ALL: &str = "/v1/auth/sessions/all";

/// The dashboard's bearer, which the fixture identity accepts.
const DASHBOARD_TOKEN: &str = "fixture.header.payload";

/// The subject the dashboard session resolves to.
const SUBJECT: &str = "user_2device_flow";

/// A well-formed approval: the payload the browser seals and hands over.
const APPROVE_BODY: &str = r#"{
    "dashboard_public_key": "fixture-dashboard-key",
    "ciphertext": "sealed-credential",
    "nonce": "fixture-nonce",
    "verification_code": "012345"
}"#;

/// The code the approval above committed to.
const VERIFY_BODY: &str = r#"{"verification_code":"012345"}"#;

#[tokio::test]
#[ignore = "needs live Redis: make test-integration-rustd"]
async fn device_sessions_open_approve_verify_replay_and_cancel() {
    let router = Fleet::new()
        .with_session_queue(harness::connect_redis().await)
        .with_dashboard(SUBJECT)
        .router();

    let redeemable = open_session(&router, "redeemable").await;
    assert_session_status(&router, &redeemable, "pending").await;
    approve_session(&router, &redeemable).await;
    assert_session_status(&router, &redeemable, "verification_pending").await;
    for _attempt in 0..2 {
        let verified = harness::send(
            &router,
            Method::POST,
            &format!("/v1/auth/sessions/{redeemable}/verify"),
            None,
            VERIFY_BODY,
        )
        .await;
        assert_eq!(verified.status(), StatusCode::OK);
        let body = harness::json_body(verified).await;
        assert_eq!(
            body.get("ciphertext").and_then(Value::as_str),
            Some("sealed-credential")
        );
    }

    let cancelled = open_session(&router, "cancelled").await;
    approve_session(&router, &cancelled).await;
    for _attempt in 0..2 {
        let response = harness::send(
            &router,
            Method::DELETE,
            &format!("/v1/auth/sessions/{cancelled}"),
            Some(DASHBOARD_TOKEN),
            "",
        )
        .await;
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    for name in ["bulk-one", "bulk-two"] {
        let session = open_session(&router, name).await;
        approve_session(&router, &session).await;
    }
    let all = harness::send(&router, Method::DELETE, ALL, Some(DASHBOARD_TOKEN), "").await;
    assert_eq!(all.status(), StatusCode::OK);
    assert_eq!(
        harness::json_body(all)
            .await
            .get("aborted_count")
            .and_then(Value::as_u64),
        Some(2)
    );
}

async fn open_session(router: &axum::Router, token_name: &str) -> String {
    let response = harness::send(
        router,
        Method::POST,
        SESSIONS,
        None,
        &format!(r#"{{"public_key":"fixture-public-key","token_name":"{token_name}"}}"#),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    harness::json_body(response)
        .await
        .get("session_id")
        .and_then(Value::as_str)
        .expect("an opened session returns its id")
        .to_owned()
}

async fn approve_session(router: &axum::Router, session: &str) {
    let response = harness::send(
        router,
        Method::PATCH,
        &format!("/v1/auth/sessions/{session}/approve"),
        Some(DASHBOARD_TOKEN),
        APPROVE_BODY,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn assert_session_status(router: &axum::Router, session: &str, expected: &str) {
    let response = harness::send(
        router,
        Method::GET,
        &format!("/v1/auth/sessions/{session}"),
        None,
        "",
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        harness::json_body(response)
            .await
            .get("status")
            .and_then(Value::as_str),
        Some(expected)
    );
}
