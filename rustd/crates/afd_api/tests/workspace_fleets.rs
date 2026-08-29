//! The workspace fleets surface's refusal matrix — everything in FRONT of the verbs.
//!
//! # What this suite is for, and what it deliberately is not
//!
//! The install writes a row, creates a Redis stream and a consumer group, and
//! flips a status; the list is one statement; the PATCH is a transaction holding
//! a row lock. None of that can be stubbed honestly, so the harness answers the
//! refusal a datastore that would not answer gives, and what these tests pin is
//! everything a request meets before either datastore is reached: the guard, the
//! scope rung, the OWNERSHIP layer, the path identifiers, and every body and
//! query refusal.
//!
//! # Dimension 3.3 lives here
//!
//! A principal with valid scopes and the wrong workspace is a 403 that never
//! runs a statement; the same principal on their OWN workspace reaches the verb.
//! Both halves are provable with no Postgres in the process, because
//! [`OneWorkspace`] answers the ownership seam honestly while every other stub
//! refuses.
//!
//! The 404 half of that axis — a fleet another workspace owns — needs a seeded
//! row to lose against, because the predicate doing the work is in the SQL.
//! That proof rides the integration lane.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tdecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2fleets";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A well-formed fleet identifier.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000f1ee";

/// The scopes each verb's route row demands.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);
const FLEET_ADMIN: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetAdmin]);

/// Every scope this surface can ask for, so a test isolating another axis is
/// never refused by the rung.
const EVERY_FLEET_SCOPE: ScopeSet =
    ScopeSet::from_scopes(&[Scope::FleetRead, Scope::FleetWrite, Scope::FleetAdmin]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// The collection path, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets")
}

/// The item path, under the workspace the fixture owns.
fn item() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}")
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
    send(EVERY_FLEET_SCOPE, method, path, Some(TENANT_KEY), body).await
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

/// Asserts the request got PAST every layer in front of the verb.
///
/// 503 with the datastore's sentence is the one answer only the STORE can
/// produce, so reaching it proves the guard, the rung and the ownership layer
/// all admitted the request.
async fn assert_reached_the_verb(response: axum::response::Response, case: &str) {
    assert_eq!(
        response.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "{case}: only the verb answers with the datastore's refusal"
    );
    assert_eq!(
        detail_of(response).await,
        afd_fleet_lifecycle::error::detail::DATABASE_UNAVAILABLE,
        "{case}: the refusal is the store's, not a layer's"
    );
}

/// Every verb on this surface, with the scope its route row demands.
fn every_verb() -> [(Method, String, ScopeSet); 5] {
    [
        (Method::GET, collection(), FLEET_READ),
        (Method::POST, collection(), FLEET_WRITE),
        (Method::GET, item(), FLEET_READ),
        (Method::PATCH, item(), FLEET_WRITE),
        (Method::DELETE, item(), FLEET_ADMIN),
    ]
}

#[tokio::test]
async fn no_verb_on_this_surface_is_anonymous() {
    for (method, path, scopes) in every_verb() {
        let response = send(scopes, method.clone(), &path, None, "").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} is not anonymous"
        );
    }
}

#[tokio::test]
async fn every_verb_is_gated_on_its_own_scope() {
    for (method, path, _needs) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &path, Some(TENANT_KEY), "").await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} without its scope is refused"
        );
    }
}

#[tokio::test]
async fn a_scoped_person_reaches_every_verb_on_their_own_workspace() {
    // The allow half of Dimension 3.3: valid scopes AND the right workspace
    // reach the store, which is the only thing left that can refuse.
    // Each body is the smallest one that gets PAST the edge's own refusals, so
    // what answers is the store and not a parse. An empty PATCH is the no-op
    // that never reaches a verb at all, which is its own test below.
    for (method, path, _scopes) in every_verb() {
        let body = match &method {
            &Method::POST => r#"{"platform_library_id":"daily-digest"}"#,
            &Method::PATCH => r#"{"status":"stopped"}"#,
            _reads_and_deletes => "",
        };
        let response = authorised(method.clone(), &path, body).await;
        assert_reached_the_verb(response, &format!("{method} {path}")).await;
    }
}

#[tokio::test]
async fn a_foreign_workspace_is_refused_before_any_statement_runs() {
    // The deny half of Dimension 3.3, and the reason ownership is a LAYER: the
    // scopes are valid and the caller is proven, and they still never reach a
    // datastore. A 403 rather than a 404 is parity — a dashboard branches on it.
    for (method, path, _scopes) in every_verb() {
        let foreign = path.replace(OWNED_WORKSPACE, FOREIGN_WORKSPACE);
        let response = authorised(method.clone(), &foreign, "").await;

        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {foreign} is not this caller's"
        );
        assert_eq!(detail_of(response).await, DETAIL_NOT_YOURS, "{method}");
    }
}

#[tokio::test]
async fn a_workspace_that_is_not_an_identifier_never_reaches_the_seam() {
    // Refused before the plane is asked, so the `::uuid` casts in the statements
    // can never be the thing that fails.
    let response = authorised(Method::GET, "/v1/workspaces/not-a-uuid/fleets", "").await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_fleet_that_is_not_an_identifier_never_reaches_the_seam() {
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/not-a-uuid");
    for method in [Method::GET, Method::PATCH, Method::DELETE] {
        let response = authorised(method.clone(), &path, "").await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "{method}");
        assert_eq!(
            detail_of(response).await,
            afd_api::handler::fleet::detail::DETAIL_FLEET_ID,
            "{method}"
        );
    }
}
