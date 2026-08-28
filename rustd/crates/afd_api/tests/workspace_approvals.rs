//! Dimension 6.1's HTTP half — everything in FRONT of the gate store.
//!
//! The race and the terminal-row rule are proven against live Postgres in
//! `afd_fleet/tests/integration_approval_inbox.rs`, because only a real
//! statement can decide a race. What is pinned here is what a request meets
//! before the store: the guard, the two SEPARATE capabilities this surface
//! splits, the ownership layer, and the decision verb.
//!
//! # Reading the queue and answering it are different capabilities
//!
//! `ApprovalRead` reaches the list and the detail; `ApprovalResolve` is what
//! the decision demands. That split is the point of the suite: a person who can
//! see what a fleet wants to do must not thereby be able to let it, and a
//! single merged scope would be invisible until somebody used it.
//!
//! # The decision moved into its own path segment
//!
//! The Zig daemon spelled it `…/approvals/{gate_id}:approve`. A router binds
//! one parameter per segment, so that form could not be told apart from the
//! detail read — and the two carry different capabilities, which one mounted
//! path cannot express. `…/approvals/{gate_id}/approve` is the served spelling.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfacefeeddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2approvals";

/// A well-formed workspace identifier that is somebody else's.
const FOREIGN_WORKSPACE: &str = "01924f4e-0000-7000-8000-0000000000ff";

/// A well-formed gate identifier the fixture addresses.
const GATE: &str = "01924f4e-0000-7000-8000-00000000a11e";

/// Seeing the queue.
const APPROVAL_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::ApprovalRead]);

/// Answering it.
const APPROVAL_RESOLVE: ScopeSet = ScopeSet::from_scopes(&[Scope::ApprovalResolve]);

/// Both, for the tests isolating an axis other than scope.
const EVERY_APPROVAL_SCOPE: ScopeSet =
    ScopeSet::from_scopes(&[Scope::ApprovalRead, Scope::ApprovalResolve]);

/// The empty set, proving a refusal is the scope rung's.
const NO_SCOPES: ScopeSet = ScopeSet::from_scopes(&[]);

/// The sentence the ownership layer refuses with.
const DETAIL_NOT_YOURS: &str = "Workspace access denied";

/// The queue, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals")
}

/// One gate, under the same workspace.
fn item() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals/{GATE}")
}

/// One decision on that gate.
fn decision(verb: &str) -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals/{GATE}/{verb}")
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

/// One fully authorised request, for the tests isolating another axis.
async fn authorised(method: Method, path: &str, body: &str) -> axum::response::Response {
    send(EVERY_APPROVAL_SCOPE, method, path, Some(TENANT_KEY), body).await
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

/// Every verb on this surface, with the scope its route row demands.
fn every_verb() -> [(Method, String, ScopeSet); 3] {
    [
        (Method::GET, collection(), APPROVAL_READ),
        (Method::GET, item(), APPROVAL_READ),
        (Method::POST, decision("approve"), APPROVAL_RESOLVE),
    ]
}

/// No verb here is reachable without a credential.
#[tokio::test]
async fn no_verb_on_this_surface_is_anonymous() {
    for (method, path, scopes) in every_verb() {
        let response = send(scopes, method.clone(), &path, None, "").await;
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{method} {path} must refuse an anonymous caller"
        );
    }
}

/// Every verb is refused by the scope rung before anything else.
#[tokio::test]
async fn every_verb_is_refused_by_the_scope_rung_before_anything_else() {
    for (method, path, _demanded) in every_verb() {
        let response = send(NO_SCOPES, method.clone(), &path, Some(TENANT_KEY), "").await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} must be refused by the rung"
        );
    }
}

/// Seeing the queue does not confer the authority to answer it.
///
/// The separation the route table calls deliberate, asserted rather than
/// assumed: a reader reaches both reads and is refused the decision.
#[tokio::test]
async fn reading_the_queue_does_not_confer_the_authority_to_answer_it() {
    let listed = send(
        APPROVAL_READ,
        Method::GET,
        &collection(),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_ne!(
        listed.status(),
        StatusCode::FORBIDDEN,
        "a reader may see the queue"
    );

    let answered = send(
        APPROVAL_READ,
        Method::POST,
        &decision("approve"),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_eq!(
        answered.status(),
        StatusCode::FORBIDDEN,
        "a reader may not answer a gate"
    );
}

/// The authority to answer DOES carry the authority to browse.
///
/// The other direction of the same split, and it is deliberate rather than
/// accidental: `HIERARCHY` pairs `ApprovalResolve` with `ApprovalRead` under
/// the comment "deciding an approval gate implies the ability to view the
/// inbox". Somebody who may answer a gate must be able to read the thing they
/// are answering, so the closure is what stops the inbox from being invisible
/// to its own approvers.
///
/// Asserted here because a hierarchy edge is exactly the kind of rule that is
/// invisible until it is missing: dropping this pair would refuse an approver
/// their own queue, and nothing else in the suite would notice.
#[tokio::test]
async fn answering_a_gate_carries_the_authority_to_browse() {
    let listed = send(
        APPROVAL_RESOLVE,
        Method::GET,
        &collection(),
        Some(TENANT_KEY),
        "",
    )
    .await;
    assert_ne!(
        listed.status(),
        StatusCode::FORBIDDEN,
        "an approver reaches the inbox through the hierarchy closure"
    );
}

/// A principal acting in somebody else's workspace runs no statement.
#[tokio::test]
async fn a_principal_in_a_foreign_workspace_is_refused_by_the_layer() {
    let paths = [
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/approvals"),
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/approvals/{GATE}"),
        format!("/v1/workspaces/{FOREIGN_WORKSPACE}/approvals/{GATE}/approve"),
    ];
    let methods = [Method::GET, Method::GET, Method::POST];

    for (path, method) in paths.iter().zip(methods) {
        let response = send(EVERY_APPROVAL_SCOPE, method, path, Some(TENANT_KEY), "").await;
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

/// A decision verb this surface does not spell never reaches the store.
///
/// The path carries the decision, so an unknown one is a malformed REQUEST and
/// not a missing gate — the caller asked for something that does not exist as a
/// verb, which is a different fix from asking about a gate that does not exist.
#[tokio::test]
async fn a_decision_the_path_does_not_spell_is_refused() {
    for verb in ["maybe", "APPROVE", "approve-later", "resolve"] {
        let response = authorised(Method::POST, &decision(verb), "").await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{verb} is not a decision"
        );
    }
}

/// A gate id that is not an identifier reads as absent, not as malformed.
///
/// The same answer a well-formed id for a gate this workspace does not hold
/// gets: a caller probing identifiers learns nothing from the difference.
#[tokio::test]
async fn a_gate_id_that_is_not_an_identifier_reads_as_absent() {
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals/not-a-uuid");
    let response = authorised(Method::GET, &path, "").await;

    let document = harness::json_body(response).await;
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_NOT_FOUND.as_str()),
        "a malformed gate id is answered as a missing gate"
    );
}

/// A body this daemon cannot read never reaches the store.
#[tokio::test]
async fn a_decision_body_that_will_not_parse_is_refused() {
    for malformed in ["{", "not json", "{\"reason\":"] {
        let response = authorised(Method::POST, &decision("deny"), malformed).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{malformed} must not reach the gate"
        );
    }
}

/// A decision with no body at all is complete.
///
/// A note is optional, so an empty body must not be refused: demanding one
/// would make the common answer the awkward one. It gets past the parse and
/// reaches the store, which the datastore's own refusal proves.
#[tokio::test]
async fn a_decision_needs_no_note() {
    let response = authorised(Method::POST, &decision("approve"), "").await;
    assert_ne!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "an absent note is not a malformed request"
    );
}

/// The templates carry only the methods they document.
#[tokio::test]
async fn the_templates_carry_only_the_methods_they_document() {
    // The queue is read, never written: a gate is raised by a fleet, not by a
    // person POSTing one.
    let posted = authorised(Method::POST, &collection(), "{}").await;
    assert_eq!(posted.status(), StatusCode::METHOD_NOT_ALLOWED);

    // A gate is never deleted: the row is the audit trail of a decision.
    let deleted = authorised(Method::DELETE, &item(), "").await;
    assert_eq!(deleted.status(), StatusCode::METHOD_NOT_ALLOWED);

    // And a decision is a POST, not a GET — reading a URL must not answer a
    // gate, which is what a GET-shaped decision would allow a link to do.
    let fetched = authorised(Method::GET, &decision("approve"), "").await;
    assert_eq!(fetched.status(), StatusCode::METHOD_NOT_ALLOWED);
}
