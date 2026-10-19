//! The message thread's guard, its two rungs, and its method set.
//!
//! One template carrying two verbs that are not the same authority: reading
//! what a fleet has been told is a read, and telling it something is a write
//! that starts a billable run. Row behaviour needs live Postgres and Dragonfly and
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

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2messages";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// Reading what a fleet has been told.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// Telling it something.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

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
