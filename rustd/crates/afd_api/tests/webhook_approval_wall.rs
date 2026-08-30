//! What the approval callback proves before it resolves anything.
//!
//! `POST /v1/webhooks/{fleet_id}/approval` is the one route in this family whose
//! secret belongs to the DEPLOYMENT rather than to a fleet: an approval callback
//! is answered before anything has looked up which fleet's gate it resolves, so
//! there is no binding to read a secret through. `verify_platform.rs` reads the
//! platform admin workspace instead, and every refusal below is that file's.
//!
//! # Why the wall is tested apart from the resolution
//!
//! This route WRITES — a resolved gate, through the same approvals service the
//! dashboard and the sweeper use. Every case here is one that must never reach
//! that service, which is exactly what makes them provable with no datastore:
//! the store is left unreachable, so a refusal that leaked through would fail
//! as a connection error rather than passing quietly. The resolution half of
//! Dimension 2.2 needs a live Postgres and lives in the integration lane.
//!
//! # The stale case is the only one Slack's scheme can answer
//!
//! `Scheme::SlackV0` is the sole scheme binding a timestamp into its signed
//! bytes, and this route is the reason it matters here: an approval replayed
//! hours later carries a signature that is still arithmetically valid.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use std::sync::Arc;

use self::harness::webhook as signed;
use self::harness::{Fleet, Scripted, json_body, send_with_headers};
use afd_core::error_code::{self, ErrorCode};
use afd_webhook::{MAX_DRIFT_SECONDS, Scheme};
use http::{Method, StatusCode};

/// An approver pressing approve, as Slack posts it.
const PAYLOAD: &str = r#"{"gate_id":"01912d4e-8f2a-7c3b-9d1e-4a5b6c7d8ea2","decision":"approve"}"#;

/// The secret this DEPLOYMENT signs approval callbacks with.
const PLATFORM_SECRET: &[u8] = b"fixture-approval-signing-secret";

/// Where one fleet's approval callbacks arrive.
fn path() -> String {
    format!("/v1/webhooks/{}/approval", signed::FLEET)
}

/// The instant this harness calls now — frozen, not the wall clock.
///
/// `SlackV0` is checked against `services.now()`, so a test signing at
/// `SystemTime::now` would be sixty-odd million seconds adrift and every case
/// here would read as stale rather than as what it meant to prove. Frozen also
/// means the window boundaries below are exact rather than racing the clock
/// between signing and verifying.
fn now() -> i64 {
    harness::frozen_unix_seconds()
}

/// A deployment holding `secret` as its approval signing key, or holding none.
fn deployment(secret: Option<&[u8]>) -> Arc<Scripted> {
    let scripted = Scripted::new();
    Arc::new(match secret {
        Some(bytes) => scripted.app_signing(bytes),
        None => scripted,
    })
}

/// The registry code a presented callback earned.
///
/// The store behind the router is never made reachable, so a case that got past
/// the wall fails here as a datastore error rather than as a passing test — the
/// assertion is that the refusal happened BEFORE anything was written.
async fn refused(ingress: &Arc<Scripted>, signature: &str, timestamp: &str) -> String {
    let router = Fleet::new()
        .with_ingress(ingress)
        .with_platform_admin(signed::id(signed::WORKSPACE))
        .router();
    let headers = signed::approval_headers(signature, timestamp);
    let response = send_with_headers(&router, Method::POST, &path(), None, PAYLOAD, &headers).await;

    let status = response.status();
    let document = json_body(response).await;
    assert!(
        status.is_client_error(),
        "a refusal is the sender's to fix, so it is a 4xx: {status} {document}"
    );
    document
        .get("error_code")
        .and_then(serde_json::Value::as_str)
        .expect("every refusal carries its registry code")
        .to_owned()
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A signature over `body` at `timestamp`, under `secret`.
fn proof(secret: &[u8], timestamp: &str, body: &str) -> String {
    signed::signature_at(Scheme::SlackV0, secret, Some(timestamp), body.as_bytes())
}

#[tokio::test]
async fn a_deployment_holding_no_signing_secret_refuses_before_it_verifies() {
    let at = now().to_string();
    let presented = proof(PLATFORM_SECRET, &at, PAYLOAD);

    assert_eq!(
        refused(&deployment(None), &presented, &at).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "without a secret there is nothing to check against, and accepting an \
         unverified callback on a public endpoint is worse than serving none"
    );
}

/// The OTHER way this deployment can hold no secret, and it is not the same way.
///
/// `proven` refuses twice with one code: once when no platform admin workspace
/// is configured at all, and again when that workspace holds no
/// `approval-signing` key. Both are `UZ-WH-020` on the wire, deliberately — a
/// sender learns only that this deployment cannot verify, never which half of
/// the configuration is missing. Asserting only one of them would leave the
/// earlier return covered by nothing, which is how it was: every case in this
/// file first passed for the wrong reason, answering `UZ-WH-020` because the
/// harness configured no admin workspace rather than because the case worked.
#[tokio::test]
async fn a_deployment_with_no_admin_workspace_refuses_before_it_reads_a_secret() {
    let at = now().to_string();
    let presented = proof(PLATFORM_SECRET, &at, PAYLOAD);
    let holding = deployment(Some(PLATFORM_SECRET));

    let router = Fleet::new().with_ingress(&holding).router();
    let headers = signed::approval_headers(&presented, &at);
    let response = send_with_headers(&router, Method::POST, &path(), None, PAYLOAD, &headers).await;

    let document = json_body(response).await;
    assert_eq!(
        document
            .get("error_code")
            .and_then(serde_json::Value::as_str)
            .expect("every refusal carries its registry code"),
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "a secret it cannot address is a secret it does not have"
    );
}

#[tokio::test]
async fn a_callback_signed_with_the_wrong_key_is_refused() {
    let at = now().to_string();
    let presented = proof(signed::WRONG_SECRET, &at, PAYLOAD);

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), &presented, &at).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_payload_changed_after_signing_is_refused() {
    let at = now().to_string();
    let presented = proof(PLATFORM_SECRET, &at, r#"{"decision":"deny"}"#);

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), &presented, &at).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "the signature covers the body, so flipping approve to deny invalidates it"
    );
}

#[tokio::test]
async fn a_signature_bound_to_another_instant_is_refused() {
    let at = now().to_string();
    let other = (now() + 1).to_string();
    let presented = proof(PLATFORM_SECRET, &other, PAYLOAD);

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), &presented, &at).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "SlackV0 signs the timestamp too, so a proof made for a different \
         instant does not verify against the one presented"
    );
}

#[tokio::test]
async fn a_callback_replayed_outside_its_window_is_refused_as_stale() {
    let stale = (now() - MAX_DRIFT_SECONDS - 1).to_string();
    let presented = proof(PLATFORM_SECRET, &stale, PAYLOAD);

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), &presented, &stale).await,
        code(error_code::WEBHOOK_TIMESTAMP_STALE),
        "the signature is arithmetically valid; freshness is the only thing \
         that refuses a callback captured and replayed hours later"
    );
}

#[tokio::test]
async fn a_callback_at_the_edge_of_the_window_is_still_fresh() {
    let edge = (now() - MAX_DRIFT_SECONDS + 1).to_string();
    let presented = proof(PLATFORM_SECRET, &edge, PAYLOAD);

    assert_ne!(
        refused(&deployment(Some(PLATFORM_SECRET)), &presented, &edge).await,
        code(error_code::WEBHOOK_TIMESTAMP_STALE),
        "one second inside the window is inside it — a boundary written as `>=` \
         where `>` was meant would refuse this and nothing else would notice"
    );
}

#[tokio::test]
async fn a_callback_carrying_no_signature_is_refused() {
    let at = now().to_string();

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), "", &at).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_malformed_signature_header_is_refused_rather_than_compared() {
    let at = now().to_string();

    assert_eq!(
        refused(&deployment(Some(PLATFORM_SECRET)), "not-a-signature", &at).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_refused_callback_never_answers_a_server_error() {
    let at = now().to_string();
    let router = Fleet::new().with_ingress(&deployment(None)).router();
    let headers = signed::approval_headers("", &at);
    let response = send_with_headers(&router, Method::POST, &path(), None, PAYLOAD, &headers).await;

    assert_ne!(
        response.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "the unreachable store behind this router must never be what answers: \
         a 500 here means the wall let the callback through to it"
    );
}
