//! What the message thread accepts, and what it refuses before a statement.
//!
//! The sibling of `fleet_messages.rs`: that suite proves the guard, the two
//! rungs and the ownership layer, and this one proves the VALUES — a page size,
//! a continuation token, and the body a steer carries.
//!
//! # Every case here ends in one of two answers
//!
//! A 400, because the request could never name a row or a message; or a 503,
//! because it could and the harness's Postgres is not there. The second is the
//! useful half: it is what proves a well-formed request got past every gate in
//! front of the store rather than being refused by one of them silently.
//!
//! The parsing itself is unit-tested beside the code, in
//! `handler/fleet/message/tests.rs`. What this adds is that the refusal
//! survives the whole layer stack and arrives as the envelope and status a
//! client branches on.
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
const TENANT_KEY: &str = "agt_tdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

/// The subject the fixture credential resolves to.
const SUBJECT: &str = "user_2messages";

/// A well-formed fleet identifier the fixture addresses.
const FLEET: &str = "01924f4e-0000-7000-8000-00000000fee7";

/// The rung the read takes.
const FLEET_READ: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetRead]);

/// The rung the steer takes.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

/// The longest message a steer may carry.
///
/// `MAX_MESSAGE_BYTES`, mirrored: the handler's constant is private, and a
/// suite that imported it could not tell a bound that moved from a bound that
/// was always this.
const MAX_MESSAGE_BYTES: usize = 8192;

/// A page size past the top of the served band.
const OVER_THE_BAND: i64 = 26;

/// One request at a fresh router holding one scoped person.
async fn send(
    scopes: ScopeSet,
    method: Method,
    path: &str,
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, scopes)
        .router();
    harness::send(&router, method, path, Some(TENANT_KEY), body).await
}

/// The thread, under the workspace the fixture owns.
fn thread() -> String {
    format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/messages")
}

/// The thread with a query string appended.
fn paged(query: &str) -> String {
    format!("{}?{query}", thread())
}

/// One fully authorised read.
async fn reading(path: &str) -> axum::response::Response {
    send(FLEET_READ, Method::GET, path, "").await
}

/// One fully authorised steer.
async fn steering(body: &str) -> axum::response::Response {
    send(FLEET_WRITE, Method::POST, &thread(), body).await
}

/// A steer body carrying `message`, already escaped by `serde`.
fn steer_of(message: &str) -> String {
    serde_json::json!({ "message": message }).to_string()
}

/// Reads a problem document's `error_code` back.
async fn code_of(response: axum::response::Response) -> String {
    field_of(response, "error_code").await
}

/// Reads one string field out of a problem document.
async fn field_of(response: axum::response::Response, name: &str) -> String {
    let document = harness::json_body(response).await;
    let carried = document.get(name).and_then(Value::as_str);
    carried
        .expect("every refusal carries the field the case reads")
        .to_owned()
}

/// A fleet id that is not an identifier never reaches a statement.
///
/// Both verbs: the write is the one that matters, because the RAW path text is
/// what would reach the stream key, and a variant spelling would open a
/// `fleet:{VARIANT}:events` no poll ever reads — a 202 whose message is never
/// delivered.
#[tokio::test]
async fn a_fleet_id_that_is_not_an_identifier_is_refused() {
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/not-a-uuid/messages");

    let read = reading(&path).await;
    assert_eq!(read.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(read).await, error_code::INVALID_REQUEST.as_str());

    let steered = send(FLEET_WRITE, Method::POST, &path, &steer_of("ship it")).await;
    assert_eq!(steered.status(), StatusCode::BAD_REQUEST);
    assert_eq!(code_of(steered).await, error_code::INVALID_REQUEST.as_str());
}

/// A page size outside the served band is refused, never clamped.
///
/// This surface's own vocabulary, and it differs from the memories listing next
/// door on purpose: every row here carries two bodies, so an over-large ask is
/// a mistake worth naming rather than a number to quietly shrink.
#[tokio::test]
async fn a_page_size_outside_the_band_is_refused_rather_than_clamped() {
    for query in [
        "limit=0",
        "limit=-5",
        "limit=abc",
        "limit=1.5",
        "limit=",
        &format!("limit={OVER_THE_BAND}"),
    ] {
        let response = reading(&paged(query)).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{query} is not a page size this surface serves"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INVALID_REQUEST.as_str(),
            "{query}: a bad page size is a bad request"
        );
    }
}

/// The limit is judged before the cursor.
///
/// A request wrong in both halves answers byte for byte what a request wrong in
/// the LIMIT alone answers. Only a rendered response carries the sentence that
/// distinguishes the two refusals, so a reordering of the two checks would
/// change what an operator is told and no unit test would notice.
#[tokio::test]
async fn the_limit_is_judged_before_the_cursor() {
    let both_wrong = reading(&paged("limit=0&starting_after=not-a-cursor")).await;
    let only_limit = reading(&paged("limit=0")).await;

    assert_eq!(both_wrong.status(), StatusCode::BAD_REQUEST);
    assert_eq!(only_limit.status(), StatusCode::BAD_REQUEST);

    // `request_id` is minted per response, so the documents are compared on
    // everything a caller acts on rather than on the whole body.
    let (wrong, limit) = (
        harness::json_body(both_wrong).await,
        harness::json_body(only_limit).await,
    );
    for field in ["error_code", "title", "detail"] {
        assert_eq!(
            wrong.get(field),
            limit.get(field),
            "a doubly-wrong request is told about the limit, not the cursor ({field})"
        );
    }
}

/// A continuation this walk did not issue is refused, never read as page one.
///
/// Silently serving page one is how a client keeps a paging bug for months.
#[tokio::test]
async fn a_continuation_this_walk_did_not_issue_is_refused() {
    for token in ["not-a-cursor", "!!!!", "MTcwMDAwMDAwMDAwMA", "abc:key"] {
        let response = reading(&paged(&format!("starting_after={token}"))).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{token} is not this walk's cursor"
        );
    }
}

/// A well-formed read reaches the store, and the outage is a 503.
///
/// RULE ECL: a datastore that will not answer is a TRANSPORT-class refusal
/// wherever it is raised, so a client backs off rather than treating it as its
/// own request being wrong.
#[tokio::test]
async fn a_well_formed_read_reaches_the_store_and_reports_the_outage() {
    let issued = "MTcwMDAwMDAwMDAwMDoxNzAwMDAwMDAwMDAwLTA";
    for query in [
        "",
        "limit=1",
        "limit=25",
        &format!("starting_after={issued}"),
    ] {
        let response = reading(&paged(query)).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "{query} must reach the store"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "{query}: the refusal is the datastore's"
        );
    }
}

/// A steer with nothing in it is refused before the parser runs.
#[tokio::test]
async fn a_steer_that_carries_no_body_is_refused() {
    let response = steering("").await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        field_of(response, "detail").await,
        "request body required",
        "an empty body is named as one, not as unreadable JSON"
    );
}

/// A body this daemon cannot read as a message is refused.
#[tokio::test]
async fn a_body_that_is_not_a_message_is_refused() {
    for body in ["{", "null", "[]", r#""ship it""#, "{}", r#"{"message":7}"#] {
        let response = steering(body).await;
        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "{body} is not a steer this surface accepts"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INVALID_REQUEST.as_str(),
            "{body}: an unreadable body is a bad request"
        );
    }
}

/// An empty message is refused, and is told apart from an empty body.
#[tokio::test]
async fn an_empty_message_is_refused_and_named_as_one() {
    let response = steering(&steer_of("")).await;
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        field_of(response, "detail").await,
        "message must not be empty"
    );
}

/// A message past the bound is refused, and the bound is on DECODED bytes.
///
/// The pair is the claim: one byte past the ceiling is refused, and a message
/// of newlines that DOUBLES in the escaped form is not — it is under the bound
/// once decoded, and the decoded text is what the runner reads.
#[tokio::test]
async fn the_message_bound_is_measured_on_the_decoded_bytes() {
    let over = steering(&steer_of(&"a".repeat(MAX_MESSAGE_BYTES + 1))).await;
    assert_eq!(over.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        field_of(over, "detail").await,
        "message must not exceed 8192 bytes"
    );

    let escaped = steering(&steer_of(&"\n".repeat(MAX_MESSAGE_BYTES))).await;
    assert_ne!(
        escaped.status(),
        StatusCode::BAD_REQUEST,
        "a message that doubles when escaped is bounded on what it decodes to"
    );
}

/// A well-formed steer reaches the store, and the outage is a 503.
///
/// The ingress check runs first, so this proves the message got past the body
/// reader and the status read is what refused — not that a steer was silently
/// accepted into a queue nobody polls.
#[tokio::test]
async fn a_well_formed_steer_reaches_the_store_and_reports_the_outage() {
    for message in [
        "ship it",
        "line one\nline \"two\"",
        "an emoji lands here \u{2728}",
        &"a".repeat(MAX_MESSAGE_BYTES),
    ] {
        let response = steering(&steer_of(message)).await;
        assert_eq!(
            response.status(),
            StatusCode::SERVICE_UNAVAILABLE,
            "a steer must reach the ingress check"
        );
        assert_eq!(
            code_of(response).await,
            error_code::INTERNAL_DB_UNAVAILABLE.as_str(),
            "the refusal is the datastore's"
        );
    }
}
