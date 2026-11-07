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
//! # The decision has its own path segment
//!
//! The decision is `…/approvals/{gate_id}/approve`, not
//! `…/approvals/{gate_id}:approve`. The reason is narrower than a capability
//! argument — `Scopes::rw` pins a different capability per method on one
//! template, so the split above is expressible either way. `matchit` simply
//! refuses any literal after a parameter inside one segment (`tree.rs:783`,
//! "Prefixes after route parameters are not supported"), so
//! `{gate_id}:approve` is not a pattern it will accept.
//! `FleetRoute::ScheduleSync` takes the same shape for the same reason.
#![cfg(feature = "test-util")]

use crate::harness;

use afd_auth::scope::{Scope, ScopeSet};
use afd_core::error_code;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{Fleet, OWNED_WORKSPACE};

/// A tenant api-key, shaped as the authenticator classifies one.
const TENANT_KEY: &str = "agt_tfacefeeddecafbaddecafbaddecafbaddecafbaddecafbaddecafbaddecafbad";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2approvals";

/// A well-formed gate identifier the fixture addresses.
const GATE: &str = "01924f4e-0000-7000-8000-00000000a11e";

/// Seeing the queue.
const APPROVAL_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::ApprovalRead]);

/// Answering it.
const APPROVAL_RESOLVE: ScopeSet = ScopeSet::from_scopes(&[Scope::ApprovalResolve]);

/// Both, for the tests isolating an axis other than scope.
const EVERY_APPROVAL_SCOPE: ScopeSet =
    ScopeSet::from_scopes(&[Scope::ApprovalRead, Scope::ApprovalResolve]);

/// The queue, under the workspace the fixture owns.
fn collection() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/approvals")
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

/// Plain and escaped notes are both valid JSON strings and reach the store.
#[tokio::test]
async fn a_decision_accepts_borrowed_and_decoded_notes() {
    for body in [
        r#"{"reason":"reviewed"}"#,
        r#"{"reason":"line one\nline two"}"#,
    ] {
        let response = authorised(Method::POST, &decision("approve"), body).await;
        assert_ne!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "a valid note must not be rejected as malformed: {body}"
        );
    }
}
