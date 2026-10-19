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

use crate::harness;
use crate::harness::{assert_reached_the_verb, detail_of};

use afd_auth::scope::{Scope, ScopeSet};
use afd_vault::MAX_DATA_BYTES;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2secrets";

/// A name the fixture addresses the item route by.
const SECRET: &str = "anthropic-prod";

/// The scopes each verb's route row demands.
const SECRET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);
const SECRET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretWrite]);

/// Every scope this surface can ask for, so a test isolating another axis is
/// never refused by the rung.
const EVERY_SECRET_SCOPE: ScopeSet =
    ScopeSet::from_scopes(&[Scope::SecretRead, Scope::SecretWrite]);

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

#[tokio::test]
async fn deleting_a_connection_by_its_name_needs_connector_write_as_well() {
    // That delete IS the connector's Disconnect, so a key scoped to secrets
    // alone must not unplug a connector through it.
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/secrets/slack");
    let refused = authorised(Method::DELETE, &path, "").await;
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    let document = harness::json_body(refused).await;
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some("UZ-AUTH-022")
    );
    assert_eq!(
        document.get("detail").and_then(Value::as_str),
        Some("Requires scope connector:write")
    );

    let both = ScopeSet::from_scopes(&[Scope::SecretWrite, Scope::ConnectorWrite]);
    let admitted = send(both, Method::DELETE, &path, Some(TENANT_KEY), "").await;
    assert_eq!(
        admitted.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "with both scopes the delete reaches the Disconnect's own store"
    );

    // A plain secret's delete still needs only the route's own scope.
    assert_reached_the_verb(
        authorised(Method::DELETE, &item(), "").await,
        "plain secret",
    )
    .await;
}

mod input;
