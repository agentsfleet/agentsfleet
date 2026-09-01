//! The workspace fleet-library pair's refusal matrix — everything in FRONT of
//! the two verbs.
//!
//! # Why the datastore's refusal is the success signal
//!
//! Both verbs read or write a row, and neither can be stubbed honestly: the
//! gallery merges two tables under one keyset order, and the onboarding fetches
//! a bundle and writes it. So the harness answers what a datastore that will
//! not answer gives, and what this pins is everything a request meets first —
//! the guard, the scope rung, the OWNERSHIP layer, the body parse, the page
//! bounds and the cursor's binding to its workspace.
//!
//! The merged order, the seek predicate and the onboarding round trip are the
//! store's outcomes. They need a live datastore and nothing grades them yet, so
//! read the cases below as the front half of this surface and not as its
//! coverage.
//!
//! # The cursor's workspace arm is the one worth reading twice
//!
//! A token minted while browsing one workspace must not seek inside another.
//! The ownership layer already refuses a caller who does not own the path's
//! workspace, so the token check is the second half: a caller who owns BOTH
//! workspaces still cannot carry a position across them, because the page a
//! cursor resumes has to be the page it was issued from.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use base64::Engine as _;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tbaddcafebaddcafebaddcafebaddcafebaddcafebaddcafebaddcafebaddcafe";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2gallery";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// What the route table demands of the gallery read.
const LIBRARY_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// What it demands of the onboarding.
const LIBRARY_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::LibraryWrite]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A body naming a repository this daemon would fetch.
const WELL_FORMED: &str = r#"{"source_kind":"github","source_ref":"agentsfleet/reviewer"}"#;

/// The gallery path for a workspace the fixture owns.
fn owned() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleet-libraries")
}

/// A request at `path`, against a fresh router.
async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, credential, body).await
}

/// Reads a problem document's registry code back.
async fn code_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal names its registry code")
        .to_owned()
}

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

/// A gallery cursor, as the handler mints one.
///
/// Spelled here rather than built through the handler's own type: the payload's
/// field ORDER is the wire contract, and a token built through that type would
/// still round-trip if the order changed under it.
fn token(workspace: &str, limit: u32) -> String {
    let json = format!(
        "{{\"v\":2,\"created_at\":1744000000000,\"tier_rank\":0,\
\"id\":\"reviewer\",\"workspace_uuid\":\"{workspace}\",\"limit\":{limit}}}"
    );
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
}

#[tokio::test]
async fn test_both_verbs_sit_behind_the_bearer_guard() {
    for method in [Method::GET, Method::POST] {
        let anonymous = send(LIBRARY_READ, method.clone(), &owned(), None, "").await;
        assert_eq!(
            anonymous.status(),
            StatusCode::UNAUTHORIZED,
            "{method} with no credential is the guard's refusal, not a 404"
        );
    }
}

#[tokio::test]
async fn test_the_scope_rung_separates_browsing_from_onboarding() {
    // Reading the gallery is not the authority to add to it. The rung answers,
    // not the handler — the pool behind the harness cannot answer anything.
    let browsing = send(
        LIBRARY_READ,
        Method::POST,
        &owned(),
        Some(TENANT_KEY),
        WELL_FORMED,
    )
    .await;
    assert_eq!(
        browsing.status(),
        StatusCode::FORBIDDEN,
        "fleet:read alone must not authorize an onboarding"
    );

    let unscoped = send(NO_SCOPES, Method::GET, &owned(), Some(TENANT_KEY), "").await;
    assert_eq!(
        unscoped.status(),
        StatusCode::FORBIDDEN,
        "no scopes, no gallery"
    );
}

#[tokio::test]
async fn test_a_workspace_this_caller_does_not_own_is_refused_by_the_ownership_layer() {
    // The first of the two isolation halves. The second is in the statements,
    // which filter on the workspace rather than trusting this layer ran.
    let foreign = format!("/v1/workspaces/{FOREIGN_WORKSPACE}/fleet-libraries");
    for (method, scopes, body) in [
        (Method::GET, LIBRARY_READ, ""),
        (Method::POST, LIBRARY_WRITE, WELL_FORMED),
    ] {
        let refused = send(scopes, method.clone(), &foreign, Some(TENANT_KEY), body).await;
        assert_eq!(
            refused.status(),
            StatusCode::FORBIDDEN,
            "{method} into somebody else's workspace"
        );
    }
}

#[tokio::test]
async fn test_both_verbs_reach_their_service_over_the_dead_pool() {
    // What "past every refusal layer" renders as over a pool that answers
    // nothing. The two differ: the gallery reports the datastore, and the
    // onboarding fails fetching its source before it ever reaches one — from
    // the dead loopback origin the harness points every import at, so reaching
    // the pipeline never means reaching GitHub.
    let browsing = send(LIBRARY_READ, Method::GET, &owned(), Some(TENANT_KEY), "").await;
    assert_eq!(
        browsing.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "only the verb answers with the datastore's refusal"
    );

    let onboarding = send(
        LIBRARY_WRITE,
        Method::POST,
        &owned(),
        Some(TENANT_KEY),
        WELL_FORMED,
    )
    .await;
    assert_ne!(
        onboarding.status(),
        StatusCode::NOT_FOUND,
        "the onboarding route resolves and reaches its pipeline"
    );
    assert_ne!(
        onboarding.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "POST is a method this template serves"
    );
}

#[tokio::test]
async fn test_a_body_this_daemon_cannot_read_never_reaches_a_pipeline() {
    for (body, expected) in [
        ("", "A request body is required"),
        ("{", "The request body is not valid JSON"),
        (
            r#"{"source_kind":"ftp","source_ref":"owner/repo"}"#,
            "source_kind must be template, upload, or github",
        ),
        (
            r#"{"source_kind":"github","source_ref":"owner/repo/extra"}"#,
            "source_ref must be 'owner/repo' for a github source",
        ),
    ] {
        let refused = send(
            LIBRARY_WRITE,
            Method::POST,
            &owned(),
            Some(TENANT_KEY),
            body,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "body {body:?}");
        assert_eq!(
            detail_of(refused).await,
            expected,
            "both planes parse through one function, so both answer this"
        );
    }
}

#[tokio::test]
async fn test_the_page_size_is_bounded_at_both_ends() {
    for raw in ["0", "101", "-1", "", "ten", "1e2"] {
        let path = format!("{}?limit={raw}", owned());
        let refused = send(LIBRARY_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "limit {raw:?}");
        assert_eq!(code_of(refused).await, "UZ-LIBRARY-003", "limit {raw:?}");
    }
}

#[tokio::test]
async fn test_a_token_this_endpoint_never_issued_is_refused_as_malformed() {
    for raw in ["!!not-base64!!", "aGVsbG8", "eyJ2IjoxfQ"] {
        let path = format!("{}?starting_after={raw}", owned());
        let refused = send(LIBRARY_READ, Method::GET, &path, Some(TENANT_KEY), "").await;
        assert_eq!(refused.status(), StatusCode::BAD_REQUEST, "token {raw:?}");
        assert_eq!(code_of(refused).await, "UZ-LIBRARY-001", "token {raw:?}");
    }
}

#[tokio::test]
async fn test_a_cursor_minted_in_another_workspace_cannot_seek_inside_this_one() {
    // The isolation arm that the ownership layer does NOT cover: this caller
    // owns the workspace in the path, and the token names a different one.
    // Resuming it would page another workspace's gallery from a position this
    // one never issued.
    let foreign = token(FOREIGN_WORKSPACE, 50);
    let path = format!("{}?starting_after={foreign}", owned());
    let refused = send(LIBRARY_READ, Method::GET, &path, Some(TENANT_KEY), "").await;

    assert_eq!(refused.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(refused).await, "UZ-LIBRARY-002");
    assert_eq!(
        detail_of(
            send(
                LIBRARY_READ,
                Method::GET,
                &format!("{}?starting_after={foreign}", owned()),
                Some(TENANT_KEY),
                "",
            )
            .await
        )
        .await,
        "starting_after was issued for a different workspace or page size"
    );
}

#[tokio::test]
async fn test_a_cursor_minted_under_another_page_size_is_refused_too() {
    // The other half of the binding. A walk resumed under a different limit is
    // a different sequence, and the boundary does not place a row in it.
    let other_size = token(OWNED_WORKSPACE, 25);
    let path = format!("{}?limit=50&starting_after={other_size}", owned());
    let refused = send(LIBRARY_READ, Method::GET, &path, Some(TENANT_KEY), "").await;

    assert_eq!(code_of(refused).await, "UZ-LIBRARY-002");
}

#[tokio::test]
async fn test_an_empty_resume_token_starts_the_walk_rather_than_refusing_it() {
    // `?starting_after=` is not a malformed cursor, it is no cursor — the same
    // reading the Zig gives it, and the difference between a first page and a
    // 400 for a client that always sends the parameter.
    let path = format!("{}?starting_after=", owned());
    let reached = send(LIBRARY_READ, Method::GET, &path, Some(TENANT_KEY), "").await;

    assert_eq!(reached.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn test_the_template_serves_exactly_two_methods() {
    for method in [Method::PUT, Method::PATCH, Method::DELETE] {
        let refused = send(
            LIBRARY_WRITE,
            method.clone(),
            &owned(),
            Some(TENANT_KEY),
            "",
        )
        .await;
        assert_eq!(
            refused.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} is not a method this template serves"
        );
    }
}
