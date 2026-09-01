//! Who may reach the model registry's four verbs at all.
//!
//! Sibling of `tenant_model_entry_input.rs`, which pins what a caller can get
//! wrong once past this file: the bearer guard, the scope rung and the method
//! router are here, the body, the path segment and the page are there.
//!
//! # Why the datastore's refusal is the success signal
//!
//! Every verb opens with a read a real Postgres would evaluate, so over the
//! harness's unreachable pool "reached the verb" renders as the 503 only the
//! SERVICE can produce. The guard, the scope rung and the method router all
//! refuse with codes of their own before any pool is touched — which is
//! exactly what makes each of them provable here, with no datastore in the
//! picture.
//!
//! What is NOT provable this way is anything decided from a row: a duplicate
//! pair, an id that resolves to nothing, an entry that is the active selection.
//! Those are the store's outcomes, they need a live datastore, and nothing
//! grades them yet — the credential crate's integration suite next door walks
//! the activation ladder, not this quad.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::Fleet;

/// The collection template.
const ENTRIES: &str = "/v1/tenants/me/models";

/// The item template, with an identifier the parse accepts.
const ENTRY: &str = "/v1/tenants/me/models/0195b4ba-8d3a-7f13-8abc-cd0000000002";

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2registry";

/// What the route table demands of the list.
const ENTRIES_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretRead]);

/// What it demands of the writes.
const ENTRIES_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::SecretWrite]);

/// The empty set, proving a refusal below is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// A body that names both fields the create requires.
const WELL_FORMED_CREATE: &str = r#"{"model_id":"claude-opus-5","secret_ref":"anthropic-prod"}"#;

/// A body that names the one field the change requires.
const WELL_FORMED_UPDATE: &str = r#"{"model_id":"claude-opus-5"}"#;

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

/// Reads a problem document's `detail` back.
async fn detail_of(response: axum::response::Response) -> String {
    let document = harness::json_body(response).await;
    document
        .get("detail")
        .and_then(Value::as_str)
        .expect("every refusal carries a detail")
        .to_owned()
}

#[tokio::test]
async fn test_every_verb_sits_behind_the_bearer_guard() {
    for (method, path) in [
        (Method::GET, ENTRIES),
        (Method::POST, ENTRIES),
        (Method::PATCH, ENTRY),
        (Method::DELETE, ENTRY),
    ] {
        let anonymous = send(ENTRIES_READ, method.clone(), path, None, "").await;
        assert_eq!(
            anonymous.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} with no credential is the guard's refusal, not a 404"
        );
    }
}

#[tokio::test]
async fn test_the_scope_rung_separates_the_list_from_the_writes() {
    // A read-scoped credential may list and may not write; an unscoped one may
    // do neither. The rung answers, not the handler — the pool behind the
    // harness cannot answer anything.
    for (method, path) in [
        (Method::POST, ENTRIES),
        (Method::PATCH, ENTRY),
        (Method::DELETE, ENTRY),
    ] {
        let scoped_read = send(ENTRIES_READ, method.clone(), path, Some(TENANT_KEY), "").await;
        assert_eq!(
            scoped_read.status(),
            StatusCode::FORBIDDEN,
            "{method} {path}: secret:read alone must not authorize a write"
        );
    }

    let unscoped = send(NO_SCOPES, Method::GET, ENTRIES, Some(TENANT_KEY), "").await;
    assert_eq!(
        unscoped.status(),
        StatusCode::FORBIDDEN,
        "no scopes, no list"
    );
}

#[tokio::test]
async fn test_every_verb_reaches_its_service_over_the_dead_pool() {
    // 503 with the datastore sentence is what "past every refusal layer"
    // renders as over a pool that answers nothing — see the module note.
    for (method, path, scopes, body) in [
        (Method::GET, ENTRIES, ENTRIES_READ, ""),
        (Method::POST, ENTRIES, ENTRIES_WRITE, WELL_FORMED_CREATE),
        (Method::PATCH, ENTRY, ENTRIES_WRITE, WELL_FORMED_UPDATE),
        (Method::DELETE, ENTRY, ENTRIES_WRITE, ""),
    ] {
        let reached = send(scopes, method.clone(), path, Some(TENANT_KEY), body).await;
        assert_eq!(
            reached.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{method} {path}: only the verb answers with the datastore's refusal"
        );
        assert_eq!(
            detail_of(reached).await,
            "Database unavailable",
            "{method} {path}: the sentence is the credential plane's outage detail"
        );
    }
}

#[tokio::test]
async fn test_the_collection_and_the_item_serve_different_method_sets() {
    // The route table's split, asserted at the edge: a PUT on either, and a
    // POST on the item, are the method router's refusal rather than a handler's.
    for (method, path) in [
        (Method::PUT, ENTRIES),
        (Method::PUT, ENTRY),
        (Method::POST, ENTRY),
        (Method::DELETE, ENTRIES),
    ] {
        let refused = send(ENTRIES_WRITE, method.clone(), path, Some(TENANT_KEY), "").await;
        assert_eq!(
            refused.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path} is not a method this template serves"
        );
    }
}
