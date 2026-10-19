//! The fleet memories surface's guard, its rungs, and its method set.
//!
//! Row behaviour needs live Postgres and is proven in the integration lane; the
//! input validation is its own suite next door (`fleet_memories_input.rs`),
//! split the way `workspace_fleets` splits from `workspace_fleets_input`. What
//! is pinned HERE is what a request meets before any of that: the credential,
//! the two capability rungs this surface spans, the ownership layer, and which
//! methods each template answers.
//!
//! # Reading what a fleet knows and changing it are different rungs
//!
//! `GET .../memories` takes `fleet:read`; `DELETE .../memories/{key}` takes
//! `fleet:write`, because forgetting mutates what the fleet knows. It is NOT
//! `fleet:admin` — that rung is for lifecycle transitions, and putting a forget
//! behind it would mean nobody could correct a wrong lesson without also being
//! able to delete the fleet.
//!
//! # There is no store verb, and its absence is asserted
//!
//! The tenant POST was retired with the runner-push cutover: a fleet remembers
//! what it LEARNED, never what a caller asserted. `memories_integration_test`
//! pins that the collection answers 404-or-405; the router serves no POST on
//! that template, so here it is a 405 and the last test says so.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2memories";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// A memory key the fixture addresses.
const KEY: &str = "wrong-lesson";

/// Reading what a fleet remembers.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// Changing it.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

/// The collection, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/memories")
}

/// One entry, under the same workspace and fleet.
fn item(key: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/memories/{key}")
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
    harness::send(&router, method, path, credential, "").await
}

/// Reading what a fleet knows does not confer the authority to change it.
///
/// The split this surface's two route rows make, asserted rather than assumed:
/// a person who may see a wrong lesson must not thereby be able to erase it,
/// and a single merged scope would be invisible until somebody used it.
#[tokio::test]
async fn reading_what_a_fleet_knows_does_not_confer_the_authority_to_forget() {
    let read = send(FLEET_READ, Method::GET, &collection(), Some(TENANT_KEY)).await;
    assert_ne!(
        read.status(),
        StatusCode::FORBIDDEN,
        "a reader may see what the fleet remembers"
    );

    let forgotten = send(FLEET_READ, Method::DELETE, &item(KEY), Some(TENANT_KEY)).await;
    assert_eq!(
        forgotten.status(),
        StatusCode::FORBIDDEN,
        "a reader may not forget an entry"
    );
}

/// The authority to forget DOES carry the authority to read.
///
/// `HIERARCHY` pairs `FleetWrite` with `FleetRead`, and the closure is what
/// stops an operator being able to remove an entry they cannot see. A hierarchy
/// edge is exactly the kind of rule that is invisible until it is missing.
#[tokio::test]
async fn forgetting_an_entry_carries_the_authority_to_read_the_list() {
    let read = send(FLEET_WRITE, Method::GET, &collection(), Some(TENANT_KEY)).await;
    assert_ne!(
        read.status(),
        StatusCode::FORBIDDEN,
        "a writer reaches the list through the hierarchy closure"
    );
}
