//! The workspace secret surface's refusal matrix — everything in FRONT of the verbs.
//!
//! # What this suite is for, and what it deliberately is not
//!
//! Every verb here writes or reads a row: the create claims a name through
//! Postgres's own uniqueness decision, the delete counts references under a row
//! lock, the list reads four columns. None of that can be stubbed honestly, so
//! the harness answers the refusal a datastore that would not answer gives, and
//! what these tests pin is everything a request meets before Postgres is
//! reached — the guard, the scope rung, the OWNERSHIP layer, and every name and
//! body refusal.
//!
//! The round trip, the never-decrypt proof and the reference lock ride the
//! integration lane, in `afd_vault`, against a live datastore.
//!
//! # Why the input refusals are in the same file here
//!
//! The fleets surface splits them across two files because it has a query
//! grammar, a cursor and five verbs. This surface has one bounded name and one
//! bounded body, and the refusals for both come from the SAME two constructors
//! whichever verb reached them — which is the property most worth pinning, and
//! it is only visible with both axes in one place.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_vault::MAX_DATA_BYTES;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2secrets";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A name the fixture addresses the item route by.
const SECRET: &str = "anthropic-prod";

/// The scopes each verb's route row demands.
const SECRET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);
const SECRET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretWrite]);

/// Every scope this surface can ask for, so a test isolating another axis is
/// never refused by the rung.
const EVERY_SECRET_SCOPE: ScopeSet =
    ScopeSet::from_scopes(&[Scope::SecretRead, Scope::SecretWrite]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// A well-formed create body, for the tests isolating another axis.
const VALID_CREATE: &str = r#"{"name":"anthropic-prod","data":{"api_key":"sk-live"}}"#;

/// A well-formed replace body, for the same reason.
const VALID_REPLACE: &str = r#"{"data":{"api_key":"sk-live"}}"#;

/// The collection path, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/secrets")
}

/// The item path, under the workspace the fixture owns.
fn item() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/secrets/{SECRET}")
}

/// One request at a fresh router holding one scoped person.
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

/// One authorised request, for the tests isolating an axis other than scope.
async fn authorised(method: Method, path: &str, body: &str) -> axum::response::Response {
    send(EVERY_SECRET_SCOPE, method, path, Some(TENANT_KEY), body).await
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

/// Reads a problem document's registry code back.
async fn code_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal carries a registry code")
        .to_owned()
}

/// Asserts the request got PAST every layer in front of the verb.
///
/// 503 with the datastore's sentence is the one answer only the STORE can
/// produce, so reaching it proves the guard, the rung, the ownership layer and
/// both constructors all admitted the request.
async fn assert_reached_the_verb(response: axum::response::Response, case: &str) {
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{case}: only the verb answers with the datastore's refusal"
    );
    assert_eq!(
        detail_of(response).await,
        afd_vault::error::detail::DATABASE_UNAVAILABLE,
        "{case}: the refusal is the store's, not a layer's"
    );
}

/// Every verb on this surface, with the scope its route row demands and a body
/// that would be accepted.
fn every_verb() -> [(Method, String, ScopeSet, &'static str); 4] {
    [
        (Method::GET, collection(), SECRET_READ, ""),
        (Method::POST, collection(), SECRET_WRITE, VALID_CREATE),
        (Method::PUT, item(), SECRET_WRITE, VALID_REPLACE),
        (Method::DELETE, item(), SECRET_WRITE, ""),
    ]
}

#[tokio::test]
async fn no_verb_on_this_surface_is_anonymous() {
    for (method, path, scopes, body) in every_verb() {
        let response = send(scopes, method.clone(), &path, None, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} answered an unauthenticated caller"
        );
    }
}

#[tokio::test]
async fn every_verb_is_refused_by_the_scope_rung_before_anything_else() {
    // A credential this daemon accepts, carrying none of the capabilities this
    // surface asks for. The refusal must be the rung's, not the store's.
    for (method, path, _demanded, body) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &path, Some(TENANT_KEY), body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} admitted a caller holding no scope"
        );
    }
}

#[tokio::test]
async fn reading_a_secret_list_does_not_confer_the_authority_to_write_one() {
    // Two rungs on one surface, and the split is the point: an operator who may
    // see WHICH credentials a workspace holds is not thereby able to replace
    // one. Only the read verb admits a read-only principal.
    let read_only = |method: Method, path: String, body: &'static str| async move {
        send(SECRET_READ, method, &path, Some(TENANT_KEY), body).await
    };

    assert_reached_the_verb(
        read_only(Method::GET, collection(), "").await,
        "list under secret:read",
    )
    .await;

    for (method, path, body) in [
        (Method::POST, collection(), VALID_CREATE),
        (Method::PUT, item(), VALID_REPLACE),
        (Method::DELETE, item(), ""),
    ] {
        let response = read_only(method.clone(), path.clone(), body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} admitted a read-only principal"
        );
    }
}

#[tokio::test]
async fn a_scoped_principal_acting_in_a_foreign_workspace_runs_no_statement() {
    // Dimension 3.3's axis, on this surface: the credential is good and the
    // capabilities are right, and the workspace is somebody else's. The
    // ownership LAYER answers, so the store is never asked.
    for (method, path, body) in [
        (
            Method::GET,
            format!("/v1/workspaces/{FOREIGN_WORKSPACE}/secrets"),
            "",
        ),
        (
            Method::POST,
            format!("/v1/workspaces/{FOREIGN_WORKSPACE}/secrets"),
            VALID_CREATE,
        ),
        (
            Method::PUT,
            format!("/v1/workspaces/{FOREIGN_WORKSPACE}/secrets/{SECRET}"),
            VALID_REPLACE,
        ),
        (
            Method::DELETE,
            format!("/v1/workspaces/{FOREIGN_WORKSPACE}/secrets/{SECRET}"),
            "",
        ),
    ] {
        let response = authorised(method.clone(), &path, body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} acted inside a workspace the caller does not own"
        );
        assert_eq!(detail_of(response).await, DETAIL_NOT_YOURS);
    }
}

#[tokio::test]
async fn a_well_formed_request_reaches_the_store_on_every_verb() {
    // The other half of every refusal above: with the guard, the rung, the
    // ownership layer and both constructors satisfied, the only thing left to
    // answer is the store.
    for (method, path, _scopes, body) in every_verb() {
        assert_reached_the_verb(
            authorised(method.clone(), &path, body).await,
            &format!("{method} {path}"),
        )
        .await;
    }
}

#[path = "workspace_secrets/input.rs"]
mod input;
