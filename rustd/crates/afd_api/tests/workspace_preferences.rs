//! Dimension 7.1 — the preference and onboarding surfaces, in front of the store.
//!
//! # What this suite pins
//!
//! Everything a request meets before Postgres: the ownership layer, the scope
//! posture these three routes carry (`NONE` — they are about the CALLER, not
//! about what the caller may reach), the closed key registry, and the value
//! bound. The refusal a key or a value earns is decided before a connection is
//! drawn, so each is provable without a datastore.
//!
//! The round trip — write a key, read it back byte for byte, watch the
//! checklist flip — rides the integration lane in `preferences_round_trip.rs`,
//! because a bag is rows and a checklist is five `EXISTS` subqueries.
//!
//! # Why the empty bag is not tested here
//!
//! "An unset bag is `{}`, never a 404" is a claim about what the STORE returns
//! for a user with no rows. This harness's store cannot answer, so the claim is
//! the integration suite's. What IS here is the half that fails open in front
//! of it: a subject with no `core.users` row is refused with a sentence, not a
//! 500, and the refusal names the person's context rather than the workspace's.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use afd_tenant::preference::MAX_PREF_VALUE_BYTES;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2prefs";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A key inside the closed registry.
const KNOWN_KEY: &str = "getting_started_dismissed";

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// These routes carry no scope requirement of their own.
///
/// A preference is a property of the CALLER, not of anything they may reach, so
/// there is no scope that would make sense to demand: a person who can act in
/// the workspace at all can say whether they dismissed a panel. The route table
/// spells this `Scopes::Always(NONE)`, and this constant is the suite reading
/// that back — a test passing a rich set would prove nothing about the rung.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A set with something in it, to prove the rung is not silently demanding one.
const SOME_SCOPE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);

/// The preference collection, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/preferences")
}

/// One preference key, under the workspace the fixture owns.
fn item(key: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/preferences/{key}")
}

/// The onboarding checklist, under the workspace the fixture owns.
fn onboarding() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/onboarding")
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

/// One authorised request, for the tests isolating an axis other than auth.
async fn authorised(method: Method, path: &str, body: &str) -> axum::response::Response {
    send(NO_SCOPES, method, path, Some(TENANT_KEY), body).await
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

/// Every verb on this surface, with a body that would be accepted.
fn every_verb() -> [(Method, String, &'static str); 3] {
    [
        (Method::GET, collection(), ""),
        (Method::PUT, item(KNOWN_KEY), "true"),
        (Method::GET, onboarding(), ""),
    ]
}

/// No verb here is reachable without a credential.
///
/// These routes demand no SCOPE, which is exactly why this test exists: a
/// surface with an empty requirement is one edit away from being a surface with
/// no requirement at all.
#[tokio::test]
async fn no_verb_on_this_surface_is_anonymous() {
    for (method, path, body) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &path, None, body).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must refuse an anonymous caller"
        );
    }
}

/// A principal acting in somebody else's workspace runs no statement.
///
/// The ownership layer answers before the handler, so the refusal names the
/// WORKSPACE — a person who reached the store would have earned the store's
/// sentence instead.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    let paths = [
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/preferences"),
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/preferences/{KNOWN_KEY}"),
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/onboarding"),
    ];
    let methods = [Method::GET, Method::PUT, Method::GET];

    for (path, method) in paths.iter().zip(methods) {
        let response = send(NO_SCOPES, method, path, Some(TENANT_KEY), "true").await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path} must be refused before the handler"
        );
        assert_eq!(
            detail_of(response).await,
            DETAIL_NOT_YOURS,
            "{path}: the refusal is the ownership layer's"
        );
    }
}

/// Holding a scope this surface never asks for changes nothing.
///
/// The routes carry `Scopes::Always(NONE)`, so a caller with an unrelated scope
/// is admitted exactly as one with none — proving the rung is not quietly
/// demanding something the route table says it does not.
#[tokio::test]
async fn an_unrelated_scope_neither_admits_nor_refuses() {
    let with_none = send(NO_SCOPES, Method::GET, &collection(), Some(TENANT_KEY), "").await;
    let with_some = send(SOME_SCOPE, Method::GET, &collection(), Some(TENANT_KEY), "").await;
    assert_eq!(
        with_none.status(),
        with_some.status(),
        "an unrelated scope must not change the verdict"
    );
}

/// A key outside the closed registry is refused at the PATH, with its own code.
///
/// Before a connection is drawn: the registry is a closed enum, so the refusal
/// needs no datastore to decide. `UZ-PREFS-001` rather than `UZ-REQ-001`,
/// because a dashboard tells "that is not a preference" from "that body is
/// malformed" by the code.
#[tokio::test]
async fn a_key_outside_the_registry_is_refused_with_its_own_code() {
    for unknown in [
        "getting_started",
        "GETTING_STARTED_DISMISSED",
        "arbitrary_key",
    ] {
        let response = authorised(Method::PUT, &item(unknown), "true").await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{unknown} must not be writable"
        );
        assert_eq!(
            code_of(response).await,
            error_code::PREF_KEY_UNKNOWN.as_str(),
            "{unknown}: the code names the key, not the body"
        );
    }
}

/// A value past one kibibyte is refused with the OTHER code.
///
/// The split is the point: `UZ-PREFS-002` says the key was fine and the value
/// was not, which is a different thing for a client to do about.
#[tokio::test]
async fn a_value_past_the_bound_is_refused_with_its_own_code() {
    let oversize = format!("\"{}\"", "x".repeat(MAX_PREF_VALUE_BYTES));
    assert!(oversize.len() > MAX_PREF_VALUE_BYTES);

    let response = authorised(Method::PUT, &item(KNOWN_KEY), &oversize).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        code_of(response).await,
        error_code::PREF_VALUE_TOO_LARGE.as_str(),
        "an oversize value is not a malformed one"
    );
}

/// A value AT the bound is not refused by the bound.
///
/// The off-by-one that would make the documented limit a lie. It gets past the
/// cap and reaches the store, which is what a datastore refusal proves.
#[tokio::test]
async fn a_value_exactly_at_the_bound_reaches_the_store() {
    // Two bytes of quotes plus the fill, landing exactly on the cap.
    let at_bound = format!("\"{}\"", "x".repeat(MAX_PREF_VALUE_BYTES - 2));
    assert_eq!(at_bound.len(), MAX_PREF_VALUE_BYTES);

    let response = authorised(Method::PUT, &item(KNOWN_KEY), &at_bound).await;
    assert_ne!(
        code_of(response).await,
        error_code::PREF_VALUE_TOO_LARGE.as_str(),
        "a value at the cap is within it"
    );
}

/// A body that is not JSON never reaches the store.
///
/// Parsed only to refuse malformed input at the boundary — the TEXT is what
/// gets stored, so this is the one place the bytes are inspected at all.
#[tokio::test]
async fn a_body_this_daemon_cannot_read_is_refused() {
    for malformed in ["{", "not json", "{\"unclosed\":"] {
        let response = authorised(Method::PUT, &item(KNOWN_KEY), malformed).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{malformed} must not be stored"
        );
    }
}

/// An empty body is told apart from a malformed one.
///
/// A preference write has no default: there is no value to store, where a fleet
/// install can fall back to `{}` because every field there is optional.
#[tokio::test]
async fn a_write_with_no_body_is_refused() {
    let response = authorised(Method::PUT, &item(KNOWN_KEY), "").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

/// The collection answers no PUT and the item answers no GET.
///
/// The shape is deliberate: the bag is written one key at a time and read whole,
/// so a PUT at the collection would be a second way to write and a GET at the
/// item a second way to read — two shapes for each job, and two places to drift.
#[tokio::test]
async fn the_templates_carry_only_the_methods_they_document() {
    let collection_put = authorised(Method::PUT, &collection(), "true").await;
    assert_eq!(
        collection_put.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "the bag is not written whole"
    );

    let item_get = authorised(Method::GET, &item(KNOWN_KEY), "").await;
    assert_eq!(
        item_get.status(),
        StatusCode::METHOD_NOT_ALLOWED,
        "one key is not read alone"
    );
}

/// Onboarding answers no write of any kind.
///
/// The checklist is DERIVED — five signals and three preference reads — so
/// there is no state here to set. A client that wants to change it writes a
/// preference.
#[tokio::test]
async fn the_checklist_is_read_only() {
    for method in [Method::PUT, Method::POST, Method::DELETE, Method::PATCH] {
        let response = authorised(method.clone(), &onboarding(), "true").await;
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} must not reach the checklist"
        );
    }
}
