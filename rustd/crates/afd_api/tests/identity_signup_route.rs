//! What the signup route proves before it opens an account.
//!
//! `POST /v1/auth/identity-events/clerk` is the only public route in this
//! daemon that CREATES a tenant, a user and a workspace, and the only proof its
//! caller offers is a signature over the body. Every case here is one that must
//! never reach the store — which is exactly what makes them provable with no
//! datastore: the fixture's pool is unreachable, so a refusal that leaked
//! through would fail as a connection error rather than passing quietly.
//!
//! What the route refuses in the event itself — its type, its addresses, their
//! verification — is `identity_signup_events.rs`, which builds the events this
//! file signs. The provisioning half — the account rows, the replay, the
//! writeback — needs a live Postgres and lives in `identity_signup_live.rs`.
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
use axum::Router;
use axum::body::Body;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use http::{HeaderName, Method, Response};

use afd_webhook::vendor::svix;

use self::harness::{Fleet, json_body, send_with_headers};
use crate::identity_signup_events::{ADA_ADDRESS, ada};

/// Where a signup event arrives.
const PATH: &str = "/v1/auth/identity-events/clerk";

/// The secret this fixture deployment verifies against.
///
/// Carries the `whsec_` prefix and a base64 body because that is what the
/// vendor's own format is — a secret without it does not parse, and a test
/// using a bare string would be proving the parse rather than the wall.
pub(crate) const SECRET: &str = "whsec_C2FVsBQIhrscChlQIMV+b5sSYspob7oD";

/// The delivery id, which is the first field of the signed payload.
const DELIVERY: &str = "msg_2fJk8Lq0PsWzXbYtRnVdEcHgMa";

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
pub(crate) fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// One correctly-signed delivery of `body` to `router`.
pub(crate) async fn deliver(router: &Router, body: &str) -> Response<Body> {
    let signature = sign(DELIVERY, now(), body);
    send_with_headers(
        router,
        Method::POST,
        PATH,
        None,
        body,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await
}

/// A correctly-signed delivery of `body`, against a configured deployment
/// whose pool answers nothing.
pub(crate) async fn signed(body: &str) -> Response<Body> {
    deliver(&Fleet::new().with_identity_secret(SECRET).router(), body).await
}

/// The registry code a refusal carries.
pub(crate) async fn refusal_code(answer: Response<Body>) -> String {
    harness::error_code(&json_body(answer).await)
        .expect("every refusal carries its registry code")
        .to_owned()
}

#[tokio::test]
async fn a_deployment_with_no_configured_secret_refuses_every_delivery() {
    // Fail-closed, and the FIRST thing the route decides. The default fixture
    // leaves the secret unset, which is the real state of a deployment that
    // never configured one.
    let router = Fleet::new().router();
    let body = ada();
    let signature = sign(DELIVERY, now(), &body);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &body,
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
        &ada(),
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
    let body = ada();
    let signature = sign(DELIVERY, now(), &body);
    let tampered = body.replace(ADA_ADDRESS, "mallory@example.test");
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
    let body = ada();
    let signature = sign(DELIVERY, now(), &body);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &body,
        &headers(
            "msg_a_different_delivery_id",
            &now().to_string(),
            &signature,
        ),
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
    let body = ada();
    let signature = sign(DELIVERY, long_ago, &body);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &body,
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

/// A secret that is SET but will not parse is unconfigured, not a bad signature.
///
/// Two different refusals share one answer here on purpose, and the pairing is
/// the thing worth locking: an absent secret and an unreadable one both mean
/// nothing was checked, so neither can be reported as a verification that
/// failed. Calling a malformed secret a bad signature would send an operator
/// hunting the sender's key when the fault is this deployment's own
/// configuration.
#[tokio::test]
async fn a_secret_this_deployment_cannot_parse_is_unconfigured_rather_than_refused() {
    let router = Fleet::new()
        .with_identity_secret("this is not a vendor secret")
        .router();
    let body = ada();
    let signature = sign(DELIVERY, now(), &body);
    let answer = send_with_headers(
        &router,
        Method::POST,
        PATH,
        None,
        &body,
        &headers(DELIVERY, &now().to_string(), &signature),
    )
    .await;

    assert_eq!(
        refusal_code(answer).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "a secret that will not parse is this deployment's own configuration \
         failing, not the sender's signature"
    );
}
