//! The message thread's guard, its two rungs, and its method set.
//!
//! One template carrying two verbs that are not the same authority: reading
//! what a fleet has been told is a read, and telling it something is a write
//! that starts a billable run. Row behaviour needs live Postgres and Redis and
//! is proven in the integration lane; the values each verb accepts are their
//! own suite next door (`fleet_messages_input.rs`), split the way
//! `workspace_fleets_input` splits from `workspace_fleets`.
//!
//! # The write rung is the one that matters here
//!
//! `GET .../messages` takes `fleet:read`; `POST .../messages` takes
//! `fleet:write`, because a steer wakes the fleet and spends the workspace's
//! credit. A single merged scope would hand every dashboard viewer the ability
//! to run an agent, and nothing would report it — which is exactly the kind of
//! rule that is invisible until somebody uses it.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2messages";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// Reading what a fleet has been told.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// Telling it something.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// A steer body every case here can send, because none of them reaches it.
const A_STEER: &str = r#"{"message":"ship it"}"#;

/// The thread, under the workspace the fixture owns.
fn thread() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/messages")
}

/// One request at a fresh router holding one scoped person.
async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    credential: Option<&str>,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, credential, A_STEER).await
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

/// Every verb on this template, with the scope its route row demands.
fn every_verb() -> [(Method, ScopeSet); 2] {
    [(Method::GET, FLEET_READ), (Method::POST, FLEET_WRITE)]
}

/// Neither verb here is reachable without a credential.
#[tokio::test]
async fn neither_verb_on_this_surface_is_anonymous() {
    for (method, scopes) in every_verb() {
        let response = send(scopes, method.clone(), &thread(), None).await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} must refuse an anonymous caller"
        );
    }
}

/// Both verbs are refused by the scope rung before anything else.
#[tokio::test]
async fn every_verb_is_refused_by_the_scope_rung_before_anything_else() {
    for (method, _demanded) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &thread(), Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} must be refused by the rung"
        );
    }
}

/// Reading a thread does not confer the authority to steer it.
///
/// The split the route row makes, asserted rather than assumed: a person who
/// may watch what a fleet is doing must not thereby be able to make it do
/// something, because a steer starts a run the workspace pays for.
#[tokio::test]
async fn reading_a_thread_does_not_confer_the_authority_to_steer_it() {
    let read = send(FLEET_READ, Method::GET, &thread(), Some(TENANT_KEY)).await;
    assert_ne!(
        read.status(),
        StatusCode::FORBIDDEN,
        "a reader may see what the fleet has been told"
    );

    let steered = send(FLEET_READ, Method::POST, &thread(), Some(TENANT_KEY)).await;
    assert_eq!(
        steered.status(),
        StatusCode::FORBIDDEN,
        "a reader may not put work on the fleet"
    );
}

/// The authority to steer DOES carry the authority to read.
///
/// `HIERARCHY` pairs `FleetWrite` with `FleetRead`, and the closure is what
/// stops an operator being able to send a message they cannot then see the
/// answer to. A hierarchy edge is invisible until it is missing.
#[tokio::test]
async fn steering_a_fleet_carries_the_authority_to_read_the_thread() {
    let read = send(FLEET_WRITE, Method::GET, &thread(), Some(TENANT_KEY)).await;
    assert_ne!(
        read.status(),
        StatusCode::FORBIDDEN,
        "a steerer reaches the thread through the hierarchy closure"
    );
}

/// A principal acting in somebody else's workspace runs no statement.
///
/// Both verbs, because the write is the one with teeth: a steer that got past
/// the layer would append to another tenant's stream and be leased by their
/// runner.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    let path = format!("/v1/workspaces/{FOREIGN_WORKSPACE}/fleets/{FLEET}/messages");
    for (method, scopes) in every_verb() {
        let response = send(scopes, method.clone(), &path, Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} must be refused before the handler"
        );
        assert_eq!(
            detail_of(response).await,
            DETAIL_NOT_YOURS,
            "{method}: the refusal is the ownership layer's"
        );
    }
}

/// The template carries only the two methods it documents.
///
/// A thread is append-only from the outside: there is no PATCH, because an
/// operator does not edit what was said, and no DELETE, because a run's history
/// is what the audit reads. Both would be a second, quieter way to change a
/// fleet's record of itself.
#[tokio::test]
async fn the_template_carries_only_the_methods_it_documents() {
    for method in [Method::PATCH, Method::PUT, Method::DELETE] {
        let response = send(FLEET_WRITE, method.clone(), &thread(), Some(TENANT_KEY)).await;
        assert_eq!(
            response.status(),
            StatusCode::METHOD_NOT_ALLOWED,
            "{method} is not served on the thread"
        );
    }
}
