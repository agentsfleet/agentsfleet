//! What the signup route refuses in a correctly-signed event, before the store.
//!
//! The signature wall is `identity_signup_route.rs`; every case here gets past
//! it and is refused, or answered and ignored, for what the event SAYS: its
//! type, which address it names primary, and whether the provider proved it.
//! The fixture's pool is unreachable, so a refusal that leaked through to the
//! store would surface as a connection error rather than as these codes.
#![cfg(feature = "test-util")]

use afd_core::error_code;
use axum::body::Body;
use http::{Response, StatusCode};
use serde_json::{Value, json};

use crate::harness::{self, json_body};
use crate::identity_signup_route::{code, refusal_code, signed};

/// The subject and address of the person most cases sign up.
const ADA: &str = "user_2fJk8Lq0";
pub(crate) const ADA_ADDRESS: &str = "ada@example.test";

/// The address id a fixture event names as its primary.
const PRIMARY: &str = "idn_1";

/// The verification status the provider writes for a proven address.
const VERIFIED: &str = "verified";

/// The event type that opens an account.
const USER_CREATED: &str = "user.created";

/// One address the provider holds, under `id`, with the verification it reports.
fn address(id: &str, email: &str, verification: &Value) -> Value {
    json!({ "id": id, "email_address": email, "verification": verification })
}

/// One address the provider proved, under the id the primary names.
fn verified(email: &str) -> Value {
    address(PRIMARY, email, &json!({ "status": VERIFIED }))
}

/// A `user.created` carrying exactly `data`.
fn event(data: &Value) -> String {
    json!({ "type": USER_CREATED, "data": data }).to_string()
}

/// A `user.created` for `id`, with one verified primary `email`.
///
/// Each key of `extra` adds to the event's `data` or replaces the default one,
/// so a case spells only the field it is about.
pub(crate) fn created(id: &str, email: &str, extra: Value) -> String {
    let mut data = json!({
        "id": id,
        "email_addresses": [verified(email)],
        "primary_email_address_id": PRIMARY,
    });
    if let (Some(fields), Value::Object(extra)) = (data.as_object_mut(), extra) {
        fields.extend(extra);
    }
    event(&data)
}

/// Ada's `user.created`, the one this daemon opens an account from.
pub(crate) fn ada() -> String {
    created(
        ADA,
        ADA_ADDRESS,
        json!({ "first_name": "Ada", "last_name": "Lovelace" }),
    )
}

/// Asserts `answer` refuses an address the provider never proved: the code is
/// the generic invalid-request one, so the detail is what names the fault.
async fn assert_unverified(answer: Response<Body>, case: &str) {
    assert_eq!(answer.status(), StatusCode::BAD_REQUEST, "{case}");
    let problem = json_body(answer).await;
    assert_eq!(
        harness::error_code(&problem),
        Some(error_code::INVALID_REQUEST.as_str()),
        "{case}"
    );
    let detail = problem.get("detail").and_then(Value::as_str);
    assert!(
        detail.is_some_and(|detail| detail.contains("not verified")),
        "{case}: the refusal names the unverified address: {problem}"
    );
}

#[tokio::test]
async fn a_verified_body_that_is_not_an_identity_event_is_refused() {
    let answer = signed(r#"{"not":"an event"}"#).await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "a verified body this route cannot read is the sender's fault"
    );
}

#[tokio::test]
async fn an_event_this_daemon_serves_no_rule_for_is_answered_rather_than_refused() {
    // 200, never a 4xx. Every one of these is a real, correctly-signed
    // delivery; answering an error would put it in the provider's retry queue
    // forever, and retrying changes nothing about the event's type.
    let answer = signed(r#"{"type":"user.updated","data":{"id":"user_2fJk8Lq0"}}"#).await;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        json_body(answer)
            .await
            .get("ignored")
            .and_then(Value::as_str),
        Some("user.updated")
    );
}

#[tokio::test]
async fn the_account_deletion_event_is_ignored_rather_than_acted_on() {
    // Deliberately NOT ported. Tearing an account down is a destructive path
    // with its own blast radius, and landing it under cover of the route that
    // OPENS accounts would ship a delete nobody reviewed. Pinned as a test so
    // the gap is a decision rather than an oversight.
    let answer = signed(r#"{"type":"user.deleted","data":{"id":"user_2fJk8Lq0"}}"#).await;
    assert_eq!(answer.status(), StatusCode::OK);
    assert_eq!(
        json_body(answer)
            .await
            .get("ignored")
            .and_then(Value::as_str),
        Some("user.deleted"),
        "an unported destructive path must answer as unhandled, never act"
    );
}

#[tokio::test]
async fn an_event_naming_no_primary_address_is_refused_before_the_store() {
    // The fixture's pool is unreachable, so a refusal that leaked through would
    // surface as a connection error rather than as this code.
    let answer = signed(&event(
        &json!({ "id": ADA, "email_addresses": [verified(ADA_ADDRESS)] }),
    ))
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST)
    );
}

#[tokio::test]
async fn an_address_the_provider_did_not_mark_primary_is_not_substituted() {
    // The one that matters most in this file. Falling back to the first address
    // in the list would open an account under whichever address happened to
    // sort first — somebody else's inbox, when a provider reports several.
    let answer = signed(&created(
        ADA,
        ADA_ADDRESS,
        json!({ "primary_email_address_id": "idn_absent" }),
    ))
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "a primary id naming no address must refuse, never fall back to the list"
    );
}

#[tokio::test]
async fn an_unverified_primary_address_is_refused_before_the_store() {
    // Accepting an invite matches on this address, so an account opened under
    // an address nobody proved would hand that address's invites to whoever
    // typed it. Every status but `verified` reads as unproven.
    for status in ["unverified", "expired", "failed", "a_status_added_later"] {
        let unproven = address(PRIMARY, ADA_ADDRESS, &json!({ "status": status }));
        let answer = signed(&created(
            ADA,
            ADA_ADDRESS,
            json!({ "email_addresses": [unproven] }),
        ))
        .await;
        assert_unverified(answer, status).await;
    }
}

#[tokio::test]
async fn a_verified_secondary_does_not_stand_in_for_an_unverified_primary() {
    let addresses = [
        address(PRIMARY, ADA_ADDRESS, &json!({ "status": "unverified" })),
        address("idn_2", "ada@personal.test", &json!({ "status": VERIFIED })),
    ];
    let answer = signed(&created(
        ADA,
        ADA_ADDRESS,
        json!({ "email_addresses": addresses }),
    ))
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "the primary decides; a verified secondary is not substituted"
    );
}

#[tokio::test]
async fn an_address_carrying_no_verification_is_refused() {
    for unproven in [
        json!({ "id": PRIMARY, "email_address": ADA_ADDRESS }),
        address(PRIMARY, ADA_ADDRESS, &Value::Null),
    ] {
        let case = unproven.to_string();
        let answer = signed(&created(
            ADA,
            ADA_ADDRESS,
            json!({ "email_addresses": [unproven] }),
        ))
        .await;
        assert_unverified(answer, &case).await;
    }
}

#[tokio::test]
async fn an_address_with_no_local_part_is_refused_rather_than_renamed() {
    // Substituting a fixed tenant name here would hide a malformed event behind
    // a tenant nobody can tell from another, so this refuses.
    let answer = signed(&created(ADA, "@example.test", json!({}))).await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST)
    );
}
