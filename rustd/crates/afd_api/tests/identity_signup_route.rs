//! What the signup route proves before it opens an account.
//!
//! `POST /v1/auth/identity-events/clerk` is the only public route in this
//! daemon that CREATES a tenant, a user and a workspace, and the only proof its
//! caller offers is a signature over the body. Every case here is one that must
//! never reach the store — which is exactly what makes them provable with no
//! datastore: the fixture's pool is unreachable, so a refusal that leaked
//! through would fail as a connection error rather than passing quietly.
//!
//! The provisioning half — the five rows, the replay, the wallet heal — needs a
//! live Postgres and lives in `integration_identity_signup.rs`.
//!
//! # Why the unconfigured case is first
//!
//! It is the first thing the route decides, before the body is read as anything
//! but bytes. A deployment that configured no secret refuses every delivery,
//! because accepting an unverified one on the route that CREATES ACCOUNTS is
//! strictly worse than serving none.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use afd_core::error_code::{self, ErrorCode};
use afd_crypto::mac::HmacSha256Tag;
use afd_crypto::secret::SecretBytes;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::{HeaderName, Method, StatusCode};
use serde_json::Value;

use afd_webhook::vendor::svix;

use self::harness::{Fleet, json_body, send_with_headers};

/// Where a signup event arrives.
const PATH: &str = "/v1/auth/identity-events/clerk";

/// The secret this fixture deployment verifies against.
///
/// Carries the `whsec_` prefix and a base64 body because that is what the
/// vendor's own format is — a secret without it does not parse, and a test
/// using a bare string would be proving the parse rather than the wall.
const SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";

/// The delivery id, which is the first field of the signed payload.
const DELIVERY: &str = "msg_2fJk8Lq0PsWzXbYtRnVdEcHgMa";

/// A `user.created` this daemon can open an account from.
const CREATED: &str = r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}],"primary_email_address_id":"idn_1","first_name":"Ada","last_name":"Lovelace"}}"#;

/// The instant this fixture signs and verifies at — frozen, not the wall clock.
fn now() -> i64 {
    harness::frozen_unix_seconds()
}

/// Signs exactly as the verifier expects, so a passing case is a round trip
/// rather than a restatement of the implementation's own output.
fn sign(id: &str, timestamp: i64, body: &str) -> String {
    let stamp = timestamp.to_string();
    let raw = STANDARD
        .decode(
            SECRET
                .strip_prefix("whsec_")
                .expect("the fixture carries the vendor's prefix"),
        )
        .expect("the fixture secret is base64");
    let tag = HmacSha256Tag::compute_peppered(
        &SecretBytes::new(raw),
        &[id.as_bytes(), b".", stamp.as_bytes(), b".", body.as_bytes()],
    );
    format!("v1,{}", STANDARD.encode(tag.as_bytes()))
}

/// The three headers a Svix delivery carries.
fn headers<'d>(id: &'d str, timestamp: &'d str, signature: &'d str) -> [(HeaderName, &'d str); 3] {
    [
        (HeaderName::from_static(svix::ID_HEADER), id),
        (HeaderName::from_static(svix::TIMESTAMP_HEADER), timestamp),
        (HeaderName::from_static(svix::SIGNATURE_HEADER), signature),
    ]
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A correctly-signed delivery of `body`, against a configured deployment.
async fn signed(body: &str) -> http::Response<axum::body::Body> {
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), body);
    send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        body,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await
}

/// The registry code a refusal carries.
async fn refusal_code(answer: http::Response<axum::body::Body>) -> String {
    json_body(answer)
        .await
        .get("error_code")
        .and_then(Value::as_str)
        .expect("every refusal carries its registry code")
        .to_owned()
}

#[tokio::test]
async fn a_deployment_with_no_configured_secret_refuses_every_delivery() {
    // Fail-closed, and the FIRST thing the route decides. The default fixture
    // leaves the secret unset, which is the real state of a deployment that
    // never configured one.
    let router = Fleet::new().router();
    let signature = sign(DELIVERY, now(), CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "an absent secret is unconfigured, never a failed verification — the \
         two are told apart by the code, which is what an operator reads"
    );
}

#[tokio::test]
async fn a_signature_under_the_wrong_key_is_refused_before_the_body_is_read() {
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let forged = format!("v1,{}", STANDARD.encode([0x11_u8; 32]));
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &now().to_string(), &forged),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_tampered_body_no_longer_verifies() {
    // The signature is taken over the ORIGINAL body and presented with an
    // altered one — the case that proves the tag covers the payload and not
    // just the headers.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), CREATED);
    let tampered = CREATED.replace("ada@example.test", "mallory@example.test");
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &tampered,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "an address swapped after signing must not open an account"
    );
}

#[tokio::test]
async fn a_delivery_resent_under_a_fresh_id_no_longer_verifies() {
    // `svix-id` is the FIRST field of the signed payload, so it is not an
    // unauthenticated hint: a captured delivery replayed under a new id fails
    // the tag rather than opening a second account.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let signature = sign(DELIVERY, now(), CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers("msg_a_different_delivery_id", &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_delivery_outside_its_window_is_stale_rather_than_forged() {
    // Two refusals an operator must be able to tell apart: somebody replaying
    // an old capture, and somebody probing with a bad key.
    let router = Fleet::new().with_identity_secret(SECRET).router();
    let long_ago = now() - ONE_DAY_SECONDS;
    let signature = sign(DELIVERY, long_ago, CREATED);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        CREATED,
        &headers(DELIVERY, &long_ago.to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_TIMESTAMP_STALE)
    );
}

/// A day in seconds — well past any freshness window this route enforces.
const ONE_DAY_SECONDS: i64 = 60 * 60 * 24;

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
        json_body(answer).await.get("ignored").and_then(Value::as_str),
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
        json_body(answer).await.get("ignored").and_then(Value::as_str),
        Some("user.deleted"),
        "an unported destructive path must answer as unhandled, never act"
    );
}

#[tokio::test]
async fn an_event_naming_no_primary_address_is_refused_before_the_store() {
    // The fixture's pool is unreachable, so a refusal that leaked through would
    // surface as a connection error rather than as this code.
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}]}}"#,
    )
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
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"ada@example.test"}],"primary_email_address_id":"idn_absent"}}"#,
    )
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST),
        "a primary id naming no address must refuse, never fall back to the list"
    );
}

#[tokio::test]
async fn an_address_with_no_local_part_is_refused_rather_than_renamed() {
    // The Zig substitutes a fixed tenant name here. That hides a malformed
    // event behind a tenant nobody can tell from another; this refuses.
    let answer = signed(
        r#"{"type":"user.created","data":{"id":"user_2fJk8Lq0","email_addresses":[{"id":"idn_1","email_address":"@example.test"}],"primary_email_address_id":"idn_1"}}"#,
    )
    .await;
    assert_eq!(
        refusal_code(answer).await,
        code(error_code::INVALID_REQUEST)
    );
}
