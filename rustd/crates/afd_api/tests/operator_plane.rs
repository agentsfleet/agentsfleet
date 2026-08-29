//! Operator-route guards and mounting through the production router.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, json_body, send};

const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_operator_surface";
const RUNNER: &str = "019329c5-0000-7000-8000-0000000000a1";

#[tokio::test]
async fn operator_reads_are_mounted_behind_their_exact_scopes() {
    let runner_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead]),
        )
        .router();
    assert_runner_reads_reach_store(&runner_reader).await;

    let fleet_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::FleetRead]),
        )
        .router();
    let bundles = send(
        &fleet_reader,
        Method::GET,
        "/v1/fleets/bundles",
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(bundles.status(), StatusCode::SERVICE_UNAVAILABLE);
}

async fn assert_runner_reads_reach_store(runner_reader: &axum::Router) {
    let reached = send(
        runner_reader,
        Method::GET,
        "/v1/fleets/runners",
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(reached)
            .await
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("UZ-INTERNAL-001")
    );

    let leases = send(
        runner_reader,
        Method::GET,
        &format!("/v1/fleets/runners/{RUNNER}/leases"),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(leases.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(leases)
            .await
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("UZ-INTERNAL-001")
    );

    let events = send(
        runner_reader,
        Method::GET,
        &format!("/v1/fleets/runners/{RUNNER}/events"),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(events.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn runner_events_reject_filter_shape_before_io() {
    let runner_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead]),
        )
        .router();
    for query in [
        "page=2",
        "page_size=10",
        "limit=0",
        "starting_after=foreign",
        "event_type=",
        "event_type=runner_online,not_an_event",
        "since=yesterday",
        "since=20&until=19",
    ] {
        let malformed = send(
            &runner_reader,
            Method::GET,
            &format!("/v1/fleets/runners/{RUNNER}/events?{query}"),
            Some(TENANT_KEY),
            "",
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body(malformed)
                .await
                .get("error_code")
                .and_then(serde_json::Value::as_str),
            Some("UZ-REQ-001")
        );
    }
}

#[tokio::test]
async fn runner_leases_require_runner_read_and_reject_query_shape_before_io() {
    let stream_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::StreamRead]),
        )
        .router();
    let denied = send(
        &stream_reader,
        Method::GET,
        &format!("/v1/fleets/runners/{RUNNER}/leases"),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let runner_reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead]),
        )
        .router();
    for query in [
        "limit=0",
        "limit=101",
        "starting_after=foreign",
        "workspace_id=workspace",
        "fleet=",
    ] {
        let malformed = send(
            &runner_reader,
            Method::GET,
            &format!("/v1/fleets/runners/{RUNNER}/leases?{query}"),
            Some(TENANT_KEY),
            "",
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body(malformed)
                .await
                .get("error_code")
                .and_then(serde_json::Value::as_str),
            Some("UZ-REQ-001")
        );
    }
}

#[tokio::test]
async fn runner_patch_is_mounted_behind_runner_write_and_rejects_shape_before_io() {
    let reader = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerRead]),
        )
        .router();
    let denied = send(
        &reader,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"action":"rotate"}"#,
    )
    .await;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);

    let writer = Fleet::new()
        .with_person(
            TENANT_KEY,
            OPERATOR,
            ScopeSet::from_scopes(&[Scope::RunnerWrite]),
        )
        .router();
    assert_runner_patch_shapes(&writer).await;
    assert_runner_patch_policy(&writer).await;
}

async fn assert_runner_patch_shapes(writer: &axum::Router) {
    for body in [
        "",
        r"{}",
        r#"{"action":"cordon","assigned_policy":{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,"extra_binds":[]}}"#,
        r#"{"action":"not_an_action"}"#,
    ] {
        let malformed = send(
            writer,
            Method::PATCH,
            &format!("/v1/fleets/runners/{RUNNER}"),
            Some(TENANT_KEY),
            body,
        )
        .await;
        assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            json_body(malformed)
                .await
                .get("error_code")
                .and_then(serde_json::Value::as_str),
            Some("UZ-REQ-001")
        );
    }
}

async fn assert_runner_patch_policy(writer: &axum::Router) {
    let unsafe_policy = send(
        writer,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"assigned_policy":{"sandbox_tier":"landlock_full","network_policy":"allow_list_egress","registry_allowlist":[],"worker_count":1,"extra_binds":[{"path":"/etc","mode":"read_write","note":"unsafe"}]}}"#,
    )
    .await;
    assert_eq!(unsafe_policy.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        json_body(unsafe_policy)
            .await
            .get("error_code")
            .and_then(serde_json::Value::as_str),
        Some("UZ-REQ-001")
    );

    let reached = send(
        writer,
        Method::PATCH,
        &format!("/v1/fleets/runners/{RUNNER}"),
        Some(TENANT_KEY),
        r#"{"action":"cordon"}"#,
    )
    .await;
    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}
