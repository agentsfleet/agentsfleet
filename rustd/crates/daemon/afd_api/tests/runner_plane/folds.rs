//! `POST /v1/runners/me/leases/{lease_id}/folds`, the claim a run makes before
//! each model call.
//!
//! The plane behind this router holds no lease and answers every claim with no
//! messages, so these prove what the route decides on its own: who may ask,
//! which bodies and lease ids it refuses before the plane is asked, and the
//! shape of the answer. Which messages a live lease claims is proven against
//! Postgres in `afd_fleet`.

use afd_auth::directory::Liveness;
use http::{Method, StatusCode};

use super::{LEASE_ID, RUNNER_TOKEN, TENANT_KEY};
use crate::harness::{Fleet, code_of, json_body, problem, runner_id, send};

/// The sentence a body this route cannot read earns.
const MALFORMED_DETAIL: &str = "Malformed folds body";

/// The sentence a cursor below zero earns.
const CURSOR_DETAIL: &str = "after_sequence must be 0 or more";

/// The sentence a path lease id that is no identifier earns.
const LEASE_DETAIL: &str = "lease_id must be a valid UUIDv7";

/// A claim from the start of the fleet's messages.
const CLAIM: &str = r#"{"fencing_token":7,"after_sequence":0}"#;

fn folds_path(lease: &str) -> String {
    format!("/v1/runners/me/leases/{lease}/folds")
}

fn runner_router() -> axum::Router {
    Fleet::new()
        .with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live)
        .router()
}

/// A body this route cannot read is refused whole, before the plane is asked,
/// and every shape that is not exactly a fence and a cursor is one.
#[tokio::test]
async fn a_folds_body_that_cannot_be_read_is_refused() {
    let router = runner_router();
    for body in [
        "",
        "{",
        "null",
        "[]",
        "{}",
        r#"{"fencing_token":7}"#,
        r#"{"after_sequence":0}"#,
        r#"{"fencing_token":-1,"after_sequence":0}"#,
        r#"{"fencing_token":7,"after_sequence":"0"}"#,
        r#"{"fencing_token":7,"after_sequence":0,"extra":1}"#,
    ] {
        let refused = send(
            &router,
            Method::POST,
            &folds_path(LEASE_ID),
            Some(RUNNER_TOKEN),
            body,
        )
        .await;
        assert_eq!(
            problem(refused).await,
            (
                StatusCode::BAD_REQUEST,
                "UZ-REQ-001".to_owned(),
                MALFORMED_DETAIL.to_owned()
            ),
            "{body:?}"
        );
    }
}

/// A cursor below zero names no message, so it is told apart from a body that
/// could not be read.
#[tokio::test]
async fn a_negative_cursor_is_refused_with_its_own_sentence() {
    let refused = send(
        &runner_router(),
        Method::POST,
        &folds_path(LEASE_ID),
        Some(RUNNER_TOKEN),
        r#"{"fencing_token":7,"after_sequence":-1}"#,
    )
    .await;
    assert_eq!(
        problem(refused).await,
        (
            StatusCode::BAD_REQUEST,
            "UZ-REQ-001".to_owned(),
            CURSOR_DETAIL.to_owned()
        )
    );
}

/// A path lease id that is no identifier is refused before the plane is asked,
/// which here would have answered with no messages.
#[tokio::test]
async fn a_lease_id_that_is_not_an_identifier_is_refused() {
    let router = runner_router();
    for lease in ["x", "not-a-lease", "01924f4e-0000-4000-8000-00000000a11e"] {
        let refused = send(
            &router,
            Method::POST,
            &folds_path(lease),
            Some(RUNNER_TOKEN),
            CLAIM,
        )
        .await;
        assert_eq!(
            problem(refused).await,
            (
                StatusCode::BAD_REQUEST,
                "UZ-REQ-001".to_owned(),
                LEASE_DETAIL.to_owned()
            ),
            "{lease}"
        );
    }
}

/// Only a presented runner credential claims: none, or a tenant key, is a 401
/// before the body is read.
#[tokio::test]
async fn a_claim_without_a_runner_credential_is_unauthorized() {
    let router = runner_router();
    for credential in [None, Some(TENANT_KEY)] {
        let refused = send(
            &router,
            Method::POST,
            &folds_path(LEASE_ID),
            credential,
            CLAIM,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{credential:?}");
        assert_eq!(code_of(refused).await, "UZ-RUN-001", "{credential:?}");
    }
}

/// A lease the runner does not hold claims nothing: an empty list, not an error,
/// so a run that lost its lease learns it from its renewal.
#[tokio::test]
async fn a_lease_the_runner_does_not_hold_answers_no_messages() {
    let router = runner_router();
    for claim in [
        CLAIM,
        r#"{"fencing_token":0,"after_sequence":9007199254740991}"#,
    ] {
        let answered = send(
            &router,
            Method::POST,
            &folds_path(LEASE_ID),
            Some(RUNNER_TOKEN),
            claim,
        )
        .await;
        assert_eq!(answered.status(), StatusCode::OK, "{claim}");
        assert_eq!(
            json_body(answered).await,
            serde_json::json!({"inputs": []}),
            "{claim}"
        );
    }
}
