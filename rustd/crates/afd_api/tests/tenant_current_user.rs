//! Who may ask `/v1/users/me` who they are.
//!
//! # Why this suite is about reaching the handler, not about the answer
//!
//! The harness holds the production store over a pool with no Postgres behind
//! it, so a 200 is unreachable here and the interesting assertion is which
//! callers get as far as the store. That is not a consolation prize on this
//! route: its whole rule is in front of the verb. Every person credential must
//! reach it, no capability may gate it, and a machine must not.
//!
//! A `503` therefore means PASSED — the request cleared admission, the
//! authenticator, the class policy and the scope gate, and died at the pool.
//! A `401` or `403` means one of those refused it, which on this route would be
//! the defect. The read's own behaviour — the join, the null display name, the
//! unknown-subject refusal — is proven against a real schema in
//! `afd_tenant`'s own integration suite.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::directory::Liveness;
use afd_auth::scope::{Scope, ScopeSet};
use axum::response::Response;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, runner_id};

/// The route under test.
const CURRENT_USER: &str = "/v1/users/me";

/// A tenant api-key — a person's credential, not a person at a terminal.
const TENANT_KEY: &str = "agt_t0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// An `afc_` credential — a person at a terminal.
const TERMINAL: &str = "afc_fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";

/// A runner token — a machine, which this route must not answer.
const RUNNER: &str = "agt_r00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

/// The subject every person fixture resolves to.
///
/// ONE subject, so any difference in outcome below is a difference in credential
/// class or capability and can be nothing else.
const SUBJECT: &str = "user_2fixture";

/// No capabilities at all — the honest fixture for a route that requires none.
///
/// This is the load-bearing one. A fixture holding scopes would pass whether or
/// not the route gates on them, so the case that proves `Scopes::Always(NONE)`
/// is a principal who holds nothing and still gets through.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A capability set, for the case that must behave identically.
const SOME_SCOPES: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// Reads a problem document's `detail` back.
async fn detail_of(response: Response) -> String {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a refusal body is small and complete");
    let document: Value = serde_json::from_slice(&bytes).expect("every refusal is problem+json");
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// Every person credential class reaches the read.
///
/// The tenant api-key is included deliberately. The command-line credential
/// routes refuse it, and this one must not: a key resolves to the person who
/// created it, so answering with that person is the honest reply, and the
/// response's `credential` field is what tells the two apart.
#[tokio::test]
async fn every_person_credential_class_reaches_the_identity_read() {
    for (label, router, token) in [
        (
            "a tenant api-key",
            Fleet::new()
                .with_person(TENANT_KEY, SUBJECT, NO_SCOPES)
                .router(),
            TENANT_KEY,
        ),
        (
            "a command-line credential",
            Fleet::new()
                .with_terminal(TERMINAL, SUBJECT, NO_SCOPES)
                .router(),
            TERMINAL,
        ),
    ] {
        let response = harness::send(&router, Method::GET, CURRENT_USER, Some(token), "").await;

        assert_ne!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{label} is a person and must be told who it is"
        );
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{label} reached the store, which in this harness has no Postgres behind it"
        );
    }
}

/// A browser session reaches it too, which is what makes it a login probe.
#[tokio::test]
async fn a_browser_session_reaches_the_identity_read() {
    let router = Fleet::new().with_dashboard(SUBJECT).router();

    let response = harness::send(&router, Method::GET, CURRENT_USER, Some("session"), "").await;

    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "the dashboard's own session must reach the read the terminal reaches"
    );
}

/// No capability gates the read, and holding one changes nothing.
///
/// The route table says `Scopes::Always(NONE)` and this is what would fail if
/// somebody added a requirement. It matters beyond tidiness: the command-line
/// client probes this route to prove a fresh login, and the route it probed
/// before required `billing:read` — so a person without that capability was
/// told their credential had been rejected.
#[tokio::test]
async fn the_identity_read_needs_no_capability() {
    for (label, scopes) in [
        ("no capabilities", NO_SCOPES),
        ("one capability", SOME_SCOPES),
    ] {
        let router = Fleet::new()
            .with_terminal(TERMINAL, SUBJECT, scopes)
            .router();

        let response = harness::send(&router, Method::GET, CURRENT_USER, Some(TERMINAL), "").await;

        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a person holding {label} reads their own identity either way"
        );
    }
}

/// A runner token is refused: this route answers for people.
#[tokio::test]
async fn a_runner_token_is_refused_the_identity_read() {
    let router = Fleet::new()
        .with_runner(RUNNER, &runner_id(), Liveness::Live)
        .router();

    let response = harness::send(&router, Method::GET, CURRENT_USER, Some(RUNNER), "").await;

    assert_ne!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "a machine must not reach a read that answers with a person"
    );
    assert!(
        response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::FORBIDDEN,
        "a runner is turned away in front of the handler, not by the store"
    );
}

/// A caller with no credential is refused, and told nothing about the route.
#[tokio::test]
async fn an_anonymous_caller_is_refused_the_identity_read() {
    let router = Fleet::new().router();

    let response = harness::send(&router, Method::GET, CURRENT_USER, None, "").await;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "the route is bearer-guarded; requiring no capability is not requiring no credential"
    );
    assert!(
        !detail_of(response).await.is_empty(),
        "a refusal carries a sentence a reader can act on"
    );
}

/// The route answers GET and nothing else.
#[tokio::test]
async fn the_identity_read_is_read_only() {
    let router = Fleet::new()
        .with_terminal(TERMINAL, SUBJECT, NO_SCOPES)
        .router();

    for method in [Method::POST, Method::PATCH, Method::DELETE, Method::PUT] {
        let response =
            harness::send(&router, method.clone(), CURRENT_USER, Some(TERMINAL), "").await;

        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} on an identity read is a method the route does not serve"
        );
    }
}
