//! What `/v1/connectors/{provider}/events` refuses before it decrypts anything.
//!
//! The return direction of the connector family: a vendor delivering events to
//! a connector this workspace installed. Two refusals sit in front of the wall,
//! and both are there for a reason a later reader could undo by tidying:
//!
//! 1. A provider with **no inbound surface** is refused before the hash and
//!    before the vault, so a path naming one costs a match rather than a
//!    decrypt — and a prober cannot measure which connectors a deployment has
//!    configured by timing the difference.
//! 2. A body over the **buffer ceiling** is refused before it is held, which is
//!    the only thing standing between a public endpoint and a sender who sends
//!    until the daemon runs out of memory.
//!
//! Everything past those two — the signature, and the handshake echoed only on
//! the far side of it — reads the connector's app secret out of the vault, so
//! it belongs to the integration lane. The handshake's own decision is already
//! proved as a unit in `afd_api_ingress::handler::events`; what is NOT provable
//! without a vault is that it is unreachable until a delivery verifies.

#![cfg(feature = "test-util")]

use crate::harness;

use self::harness::Fleet;
use af_core::error_code;
use afd_http::body::DEFAULT_MAX_REQUEST_BODY_BYTES;
use http::{Method, StatusCode};

/// A provider that delivers events to this daemon.
const DELIVERING: &str = "slack";

/// A connector this daemon ships that sends no inbound events.
///
/// The distinction the first refusal makes is not "unknown provider" but "known
/// provider, no inbound surface", and picking a provider that does not exist at
/// all would pass this test without ever reaching that branch.
const NO_EVENT_INGRESS: &str = "jira";

/// A handshake, as Slack opens a subscription with.
const HANDSHAKE: &str = r#"{"type":"url_verification","challenge":"3eZbrw1aB"}"#;

fn path(provider: &str) -> String {
    format!("/v1/connectors/{provider}/events")
}

/// One unsigned delivery of `body` to `provider`.
async fn deliver(provider: &str, body: &str) -> axum::response::Response {
    let router = Fleet::new().router();
    harness::send(&router, Method::POST, &path(provider), None, body).await
}

#[tokio::test]
async fn a_connector_with_no_inbound_surface_is_refused_before_the_vault() {
    assert_eq!(
        harness::refusal_code(deliver(NO_EVENT_INGRESS, HANDSHAKE).await).await,
        error_code::CONNECTOR_UNKNOWN.as_str(),
        "a connector this daemon ships but receives nothing from must answer \
         the same as one it does not ship, and answer it without a decrypt"
    );
}

/// A body past `[server] max_request_body_mib` is refused 413 before the
/// handler buffers it, so an unauthenticated sender cannot make this daemon
/// hold or hash more than the deployment allows.
#[tokio::test]
async fn a_body_past_the_request_body_bound_is_refused_before_the_handler() {
    let absurd = "x".repeat(DEFAULT_MAX_REQUEST_BODY_BYTES + 1);
    let router = Fleet::new().router();
    let response = harness::send(&router, Method::POST, &path(DELIVERING), None, &absurd).await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn an_unsigned_handshake_is_not_echoed() {
    let response = deliver(DELIVERING, HANDSHAKE).await;

    assert_ne!(
        response.status(),
        StatusCode::OK,
        "the challenge is echoed only past the wall. An unverified echo would \
         confirm the path to anyone who guessed it and let a prober use this \
         daemon to reflect bytes of their choosing."
    );
    let document = harness::json_body(response).await;
    assert!(
        !format!("{document}").contains("3eZbrw1aB"),
        "the challenge value must not appear in a refusal either: {document}"
    );
}
