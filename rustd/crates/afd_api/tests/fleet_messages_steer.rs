//! What a steer's BODY must be, before the thread will carry it.
//!
//! Split from `fleet_messages_input.rs` at the length cap, along the seam that
//! file already had: it proves the values a READ is asked for — a page size, a
//! continuation token — and this proves the values a WRITE carries. Both share
//! the harness router and the two answers the sibling's header describes, a 400
//! for a request that could never name a row and a 503 for one that reached the
//! store.
//!
//! The parsing itself is unit-tested beside the code, in
//! `handler/fleet/message/tests.rs`. What this adds is that the refusal
//! survives the whole layer stack and arrives as the envelope and status a
//! client branches on.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

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

/// The rung the steer takes.
const FLEET_WRITE: ScopeSet = ScopeSet::from_scopes(&[Scope::FleetWrite]);

/// The longest message a steer may carry.
///
/// `MAX_MESSAGE_BYTES`, mirrored: the handler's constant is private, and a
/// suite that imported it could not tell a bound that moved from a bound that
/// was always this.
const MAX_MESSAGE_BYTES: usize = 8192;

/// The longest operation id a steer may carry.
///
/// Mirrored for the reason `MAX_MESSAGE_BYTES` above is: importing the wire
/// type's constant would make a bound that MOVED read the same as a bound that
/// was always this, and the sentence below is a public one.
const MAX_OPERATION_ID_BYTES: usize = 200;

/// The sentence an operation id outside that bound earns.
const OPERATION_ID_DETAIL: &str = "operation_id must be between 1 and 200 bytes when present";

/// One fully authorised steer at a fresh router holding one scoped person.
async fn steering(body: &str) -> axum::response::Response {
    let router = Fleet::new()
        .with_person(TENANT_KEY, SUBJECT, FLEET_WRITE)
        .router();
    let path = format!("/v1/workspaces/{OWNED_WORKSPACE}/fleets/{FLEET}/messages");
    harness::send(&router, Method::POST, &path, Some(TENANT_KEY), body).await
}

/// A steer body carrying `message`, already escaped by `serde`.
fn steer_of(message: &str) -> String {
    serde_json::json!({ "message": message }).to_string()
}

/// A steer body carrying an operation id alongside its message.
fn steer_with(operation_id: &str, message: &str) -> String {
    serde_json::json!({ "message": message, "operation_id": operation_id }).to_string()
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
        .expect("every refusal names the field this suite reads")
        .to_owned()
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

/// An unusable operation id is refused, and is reported BEFORE the message.
///
/// The ordering is the claim, not a detail of it. One report carries all three
/// mistakes and the handler reads the operation id off it first, so a body that
/// is wrong in two ways at once is the only case that tells a correct ordering
/// from a reversed one: a caller told their message was empty would go fixing
/// the wrong field.
#[tokio::test]
async fn an_unusable_operation_id_is_refused_ahead_of_the_message() {
    for id in [String::new(), "o".repeat(MAX_OPERATION_ID_BYTES + 1)] {
        let sound_message = steering(&steer_with(&id, "ship it")).await;
        assert_eq!(sound_message.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            field_of(sound_message, "detail").await,
            OPERATION_ID_DETAIL,
            "the id is judged even where the message is fine"
        );

        let empty_message = steering(&steer_with(&id, "")).await;
        assert_eq!(
            field_of(empty_message, "detail").await,
            OPERATION_ID_DETAIL,
            "two mistakes in one body answer the id, which is read first"
        );
    }
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
