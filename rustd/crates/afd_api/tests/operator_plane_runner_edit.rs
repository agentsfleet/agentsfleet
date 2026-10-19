//! The runner patch's label edit, guarded through the production router.
//!
//! No datastore: the harness store answers 503 to any read, so a 503 is the
//! proof a request got past every guard and every bound and reached the store,
//! and any other refusal is proof it never did.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_runner::validate::DETAIL_LABELS;
use afd_wire::runner::{LABEL_MAX_BYTES, LABELS_MAX};
use http::{Method, StatusCode};
use serde_json::json;

use self::harness::{Fleet, send};

const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const OPERATOR: &str = "user_operator_runner_edit";
const RUNNER_PATH: &str = "/v1/fleets/runners/019329c5-0000-7000-8000-0000000000a1";
const LABELS_ONLY: &str = r#"{"labels":["gpu","linux","gpu"]}"#;
const POLICY: &str = r#"{"sandbox_tier":"dev_none","network_policy":"allow_all","registry_allowlist":[],"worker_count":1,"extra_binds":[]}"#;
const DETAIL_PATCH_BODY: &str = "PATCH body must be {\"action\":\"cordon|drain|revoke|rotate|self_test\"} alone, or an edit carrying {\"assigned_policy\":{sandbox_tier, network_policy, registry_allowlist[], worker_count, extra_binds[]}}, {\"labels\":[]}, or both";

fn operator(scopes: &[Scope]) -> axum::Router {
    Fleet::new()
        .with_person(TENANT_KEY, OPERATOR, ScopeSet::from_scopes(scopes))
        .router()
}

async fn patch(
    router: &axum::Router,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    send(router, Method::PATCH, RUNNER_PATH, credential, body).await
}

fn too_many() -> String {
    let labels: Vec<String> = (0..=LABELS_MAX).map(|index| format!("l{index}")).collect();
    json!({ "labels": labels }).to_string()
}

fn too_long() -> String {
    json!({ "labels": ["l".repeat(LABEL_MAX_BYTES + 1)] }).to_string()
}

fn both(labels: &str) -> String {
    format!(r#"{{"assigned_policy":{POLICY},"labels":{labels}}}"#)
}

/// No credential is a 401 whatever the body: the guard answers before the
/// body is read, so an invalid label set cannot turn it into a 400.
#[tokio::test]
async fn an_unauthenticated_label_edit_is_refused_before_its_body() {
    let router = operator(&[Scope::RunnerWrite]);
    for body in [LABELS_ONLY.to_owned(), too_many(), both(r#"["gpu"]"#)] {
        let refused = patch(&router, None, &body).await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "body={body}");
    }
}

/// Reading runners, enrolling them or administering fleets does not let a
/// caller edit labels; only `runner:write` does.
#[tokio::test]
async fn a_label_edit_without_runner_write_is_forbidden() {
    for scopes in [
        &[Scope::RunnerRead][..],
        &[Scope::RunnerEnroll][..],
        &[Scope::FleetAdmin][..],
    ] {
        let router = operator(scopes);
        for body in [LABELS_ONLY.to_owned(), too_many()] {
            let refused = patch(&router, Some(TENANT_KEY), &body).await;
            assert_eq!(refused.status(), StatusCode::FORBIDDEN, "scopes={scopes:?}");
        }
    }
}

/// Labels past either bound are refused with enrolment's own sentence, alone
/// or beside a valid policy, before the store is asked: the bad half sinks the
/// whole edit rather than letting the policy through.
#[tokio::test]
async fn invalid_labels_are_refused_with_the_enrolment_sentence_before_io() {
    let router = operator(&[Scope::RunnerWrite]);
    let past_cap: Vec<String> = (0..=LABELS_MAX).map(|index| format!("l{index}")).collect();
    let past_cap = json!(past_cap).to_string();
    for body in [too_many(), too_long(), both(&past_cap)] {
        let refused = patch(&router, Some(TENANT_KEY), &body).await;
        let (status, code, detail) = harness::problem(refused).await;
        assert_eq!(
            (status, code.as_str(), detail.as_str()),
            (StatusCode::BAD_REQUEST, "UZ-REQ-001", DETAIL_LABELS),
            "body={body}"
        );
    }
}

/// An action never rides with an edit, and a body that is not the label shape
/// is one malformed refusal, never a silent clear.
#[tokio::test]
async fn an_action_beside_labels_and_a_misshapen_label_body_are_malformed() {
    let router = operator(&[Scope::RunnerWrite]);
    for body in [
        r#"{"action":"cordon","labels":["gpu"]}"#.to_owned(),
        format!(r#"{{"action":"cordon","assigned_policy":{POLICY},"labels":[]}}"#),
        r#"{"action":"rotate","labels":[]}"#.to_owned(),
        r#"{"lables":["gpu"]}"#.to_owned(),
        r#"{"labels":[1]}"#.to_owned(),
        r#"{"labels":"gpu"}"#.to_owned(),
        r#"{"labels":null}"#.to_owned(),
    ] {
        let refused = patch(&router, Some(TENANT_KEY), &body).await;
        let (status, code, detail) = harness::problem(refused).await;
        assert_eq!(
            (status, code.as_str(), detail.as_str()),
            (StatusCode::BAD_REQUEST, "UZ-REQ-001", DETAIL_PATCH_BODY),
            "body={body}"
        );
    }
}

/// A labels-only edit, a clear, a set at both edges and a policy-plus-labels
/// save all pass every guard and reach the store.
#[tokio::test]
async fn a_valid_label_edit_under_runner_write_reaches_the_store() {
    let router = operator(&[Scope::RunnerWrite]);
    let at_edge = json!({
        "labels": (0..LABELS_MAX).map(|_| "l".repeat(LABEL_MAX_BYTES)).collect::<Vec<_>>()
    })
    .to_string();
    for body in [
        LABELS_ONLY.to_owned(),
        r#"{"labels":[]}"#.to_owned(),
        at_edge,
        both(r#"["gpu","gpu"]"#),
    ] {
        let reached = patch(&router, Some(TENANT_KEY), &body).await;
        assert_eq!(
            reached.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "body={body}"
        );
    }
}
