//! Parsing and response shapes owned by the small runner handlers.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::{FLEET_ID, LEASE_ID, RUNNER_TOKEN, code_of};
use crate::harness::{Fleet, json_body, runner_id, send};

#[tokio::test]
async fn runner_lease_satellite_routes_parse_and_render() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    let renew_path = format!("/v1/runners/me/leases/{LEASE_ID}/renew");
    for body in [
        "",
        "not-json",
        r#"{"input_tokens":1,"cached_input_tokens":2,"output_tokens":3}"#,
    ] {
        let renewed = send(&router, Method::POST, &renew_path, Some(RUNNER_TOKEN), body).await;
        assert_eq!(renewed.status(), StatusCode::OK);
        let body = json_body(renewed).await;
        assert_eq!(
            body.get("lease_expires_at"),
            Some(&serde_json::json!(1_760_000_000_000_i64))
        );
    }

    let mint_path = "/v1/runners/me/credentials/mint";
    let malformed = send(&router, Method::POST, mint_path, Some(RUNNER_TOKEN), "{}").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let unconfigured = send(
        &router,
        Method::POST,
        mint_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","integration":"github","scope":null}}"#),
    )
    .await;
    assert_eq!(unconfigured.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(code_of(unconfigured).await, "UZ-CRED-002");
}

#[tokio::test]
async fn runner_memory_routes_validate_and_render() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();

    for method in [Method::GET, Method::POST] {
        let malformed = send(
            &router,
            method,
            "/v1/runners/me/memory/not-a-uuid",
            Some(RUNNER_TOKEN),
            "{}",
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(code_of(malformed).await, "UZ-REQ-001");
    }

    let memory_path = format!("/v1/runners/me/memory/{FLEET_ID}");
    let hydrated = send(&router, Method::GET, &memory_path, Some(RUNNER_TOKEN), "").await;
    assert_eq!(hydrated.status(), StatusCode::OK);
    // A fleet without grants hydrates in the shape every runner parses:
    // `shared` and `publish` stay off the wire.
    assert_eq!(json_body(hydrated).await, serde_json::json!({"memory": []}));

    let recall_path = format!("{memory_path}/recall");
    let unbounded = send(
        &router,
        Method::POST,
        &recall_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":1,"query":"x","limit":0}}"#),
    )
    .await;
    assert_eq!(
        unbounded.status(),
        StatusCode::BAD_REQUEST,
        "a zero limit is refused"
    );
    let recalled = send(
        &router,
        Method::POST,
        &recall_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":1,"query":"x","limit":5}}"#),
    )
    .await;
    assert_eq!(recalled.status(), StatusCode::OK);
    assert_eq!(
        json_body(recalled).await,
        serde_json::json!({"memory": [], "shared": []})
    );

    let malformed = send(
        &router,
        Method::POST,
        &memory_path,
        Some(RUNNER_TOKEN),
        "{}",
    )
    .await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let captured = send(
        &router,
        Method::POST,
        &memory_path,
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":7,"memory":[]}}"#),
    )
    .await;
    assert_eq!(captured.status(), StatusCode::OK);
    assert_eq!(
        json_body(captured).await,
        serde_json::json!({"stored": 0, "skipped": 0})
    );
}

/// A 409 names the state that refuses it: a memory verb fenced out says
/// `superseded`, and a mint the GitHub App refuses says `reconnect_required`.
/// The plane decides the refusal; the route decides how it renders.
#[tokio::test]
async fn runner_conflicts_name_the_state_that_refuses_them() {
    let fenced = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_lease_refusal(afd_fleet::Error::stale_fence)
        .router();
    let memory_path = format!("/v1/runners/me/memory/{FLEET_ID}");
    let recall_path = format!("{memory_path}/recall");
    let recall = format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":1,"query":"x","limit":5}}"#);
    let capture = format!(r#"{{"lease_id":"{LEASE_ID}","fencing_token":7,"memory":[]}}"#);
    for (method, path, body) in [
        (Method::GET, &memory_path, ""),
        (Method::POST, &recall_path, recall.as_str()),
        (Method::POST, &memory_path, capture.as_str()),
    ] {
        let refused = send(&fenced, method.clone(), path, Some(RUNNER_TOKEN), body).await;
        assert_eq!(refused.status(), StatusCode::CONFLICT, "{method} {path}");
        let document = json_body(refused).await;
        assert_eq!(document["error_code"], "UZ-RUN-005", "{method} {path}");
        assert_eq!(document["current_state"], "superseded", "{method} {path}");
    }

    let revoked = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .with_lease_refusal(afd_fleet::Error::github_reconnect_required)
        .router();
    let minted = send(
        &revoked,
        Method::POST,
        "/v1/runners/me/credentials/mint",
        Some(RUNNER_TOKEN),
        &format!(r#"{{"lease_id":"{LEASE_ID}","integration":"github","scope":null}}"#),
    )
    .await;
    assert_eq!(minted.status(), StatusCode::CONFLICT);
    let document = json_body(minted).await;
    assert_eq!(document["error_code"], "UZ-GH-001");
    assert_eq!(document["current_state"], "reconnect_required");
}

/// A batch of live-tail frames is acknowledged with `{"ok":true}`, not a bare
/// 202.
///
/// A generated client types a 202 with no content as returning nothing, so the
/// assertion is on the bytes, not the status alone.
#[tokio::test]
async fn runner_activity_is_acknowledged_with_a_body() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let path = format!("/v1/runners/me/leases/{LEASE_ID}/activity");

    let accepted = send(
        &router,
        Method::POST,
        &path,
        Some(RUNNER_TOKEN),
        r#"{"frames":[{"fleet_response_chunk":{"text":"hello"}}]}"#,
    )
    .await;

    assert_eq!(accepted.status(), StatusCode::ACCEPTED);
    let body = json_body(accepted).await;
    assert_eq!(
        body,
        serde_json::json!({"ok": true}),
        "a runner pointed at either daemon reads one acknowledgement shape"
    );
}

#[tokio::test]
async fn runner_tool_call_records_validate_and_render() {
    use afd_wire::tool_detail::DETAIL_POST_MAX_BYTES;
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let path = format!("/v1/runners/me/leases/{LEASE_ID}/tool-calls");

    let malformed = send(&router, Method::POST, &path, Some(RUNNER_TOKEN), "{}").await;
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(malformed).await, "UZ-REQ-001");

    let oversized = format!(
        r#"{{"fencing_token":7,"calls":[],"pad":"{}"}}"#,
        "a".repeat(DETAIL_POST_MAX_BYTES)
    );
    let refused = send(&router, Method::POST, &path, Some(RUNNER_TOKEN), &oversized).await;
    assert_eq!(refused.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(code_of(refused).await, "UZ-REQ-002");

    let kept = send(
        &router,
        Method::POST,
        &path,
        Some(RUNNER_TOKEN),
        r#"{"fencing_token":7,"calls":[1,2]}"#,
    )
    .await;
    assert_eq!(kept.status(), StatusCode::OK);
    assert_eq!(
        json_body(kept).await,
        serde_json::json!({"stored_count": 2, "skipped_count": 0})
    );
}

/// A plane holding no lease refuses every schedule and message verb as a lease
/// this runner does not hold, before any body field is acted on.
#[tokio::test]
async fn runner_schedule_and_message_verbs_refuse_a_lease_not_held() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let schedules = format!("/v1/runners/me/leases/{LEASE_ID}/schedules");
    let messages = format!("/v1/runners/me/leases/{LEASE_ID}/messages");
    let listed = format!("{schedules}?fencing_token=1");
    let created = r#"{"fencing_token":1,"cron":"0 9 * * 1","message":"weekly check"}"#;
    let posted = r#"{"fencing_token":1,"text":"halfway there"}"#;

    for (method, path, body) in [
        (Method::GET, &listed, ""),
        (Method::POST, &schedules, created),
        (Method::POST, &messages, posted),
    ] {
        let refused = send(&router, method.clone(), path, Some(RUNNER_TOKEN), body).await;
        assert_eq!(refused.status(), StatusCode::NOT_FOUND, "{method} {path}");
        assert_eq!(code_of(refused).await, "UZ-RUN-006", "{method} {path}");
    }
}

/// The poll is answered whatever its body says about the runner's holds: an
/// empty, unreadable or out-of-bounds list reads as holding nothing, and a host
/// must not be able to fail its own poll by sending one.
#[tokio::test]
async fn runner_lease_poll_is_answered_whatever_its_body_holds() {
    let router = Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router();
    let held = format!(r#"{{"holds":["{FLEET_ID}"]}}"#);

    for body in ["", "not-json", r#"{"holds":["not-a-fleet"]}"#, &held] {
        let polled = send(
            &router,
            Method::POST,
            afd_wire::paths::RUNNER_LEASES,
            Some(RUNNER_TOKEN),
            body,
        )
        .await;
        assert_eq!(polled.status(), StatusCode::OK, "{body}");
    }
}

/// The fleets a poll's body names reach the lease plane as the runner's holds,
/// and a body naming none hands it none.
#[tokio::test]
async fn runner_lease_poll_hands_its_holds_to_the_plane() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let plane = fleet.lease_plane();
    let router = fleet.router();
    let held = format!(r#"{{"holds":["{FLEET_ID}"]}}"#);

    for body in [held.as_str(), ""] {
        let polled = send(
            &router,
            Method::POST,
            afd_wire::paths::RUNNER_LEASES,
            Some(RUNNER_TOKEN),
            body,
        )
        .await;
        assert_eq!(polled.status(), StatusCode::OK, "{body}");
    }

    assert_eq!(
        plane.polled_holds(),
        [vec![FLEET_ID.to_owned()], Vec::new()]
    );
}
