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
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use self::harness::Fleet;
use afd_api_ingress::BUFFER_CEILING;
use afd_core::error_code::{self, ErrorCode};
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

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// One unsigned delivery of `body` to `provider`.
async fn deliver(provider: &str, body: &str) -> axum::response::Response {
    let router = Fleet::new().router();
    harness::send(&router, Method::POST, &path(provider), None, body).await
}

/// The registry code a response carries, asserting it refused at all.
async fn refusal(response: axum::response::Response) -> String {
    let status = response.status();
    let document = harness::json_body(response).await;
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

#[tokio::test]
async fn a_connector_with_no_inbound_surface_is_refused_before_the_vault() {
    assert_eq!(
        refusal(deliver(NO_EVENT_INGRESS, HANDSHAKE).await).await,
        code(error_code::CONNECTOR_UNKNOWN),
        "a connector this daemon ships but receives nothing from must answer \
         the same as one it does not ship, and answer it without a decrypt"
    );
}

/// The two caps are two different refusals, and the GAP between them is why.
///
/// `BUFFER_CEILING` is the transport limit: the most this daemon will buffer
/// before an unauthenticated request can make it hold arbitrary memory.
/// `MAX_BODY_SIZE` is the semantic cap, half of it, and a body past THAT earns
/// `UZ-WH-030` with a sentence naming the limit. The doubling exists so the
/// coded answer is the one a real sender meets.
///
/// Nothing asserted the gap, and the file that documents it names exactly what
/// its loss would look like: *"Setting them equal would collapse the second
/// into the first — every over-cap delivery would earn `axum`'s bare 413
/// instead of `UZ-WH-030`, and a sender reading its delivery log would see a
/// status with no registry code to search."* A body of exactly `BUFFER_CEILING`
/// bytes is the probe for that: the layer admits it, and the handler must still
/// refuse it in the daemon's own voice. Collapse the gap and this body falls
/// under the semantic cap instead, and the case fails.
#[tokio::test]
async fn a_body_inside_the_buffer_but_past_the_cap_is_refused_in_this_daemons_voice() {
    let over_the_cap = "x".repeat(BUFFER_CEILING);

    assert_eq!(
        refusal(deliver(DELIVERING, &over_the_cap).await).await,
        code(error_code::WEBHOOK_PAYLOAD_TOO_LARGE),
        "a sender reading its delivery log needs a registry code to search for"
    );
}

#[tokio::test]
async fn a_body_past_the_buffer_is_stopped_by_the_transport_before_the_handler() {
    let absurd = "x".repeat(BUFFER_CEILING + 1);
    let router = Fleet::new().router();
    let response = harness::send(&router, Method::POST, &path(DELIVERING), None, &absurd).await;

    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "past the buffer there is no coded answer to give, because giving one          would mean holding the body to answer about it — which is the thing          the transport limit exists to refuse"
    );
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
