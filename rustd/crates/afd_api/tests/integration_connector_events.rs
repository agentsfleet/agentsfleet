//! What a connector delivery earns once its signature has been checked.
//!
//! `connector_events_route.rs` proves the two refusals that happen in FRONT of
//! the wall — a connector with no inbound surface, and a body past the cap —
//! and proves them with no datastore because neither may reach one. The wall
//! itself reads the `<provider>-app` bag's signing secret out of the platform
//! admin workspace, so everything from the signature onward needs a live vault
//! and lives here.
//!
//! # The echo is the whole reason the order matters
//!
//! A handshake asks this daemon to reflect a value the sender chose. Answering
//! one before the signature would confirm the path exists to anybody who
//! guessed it and would let a prober use this endpoint to reflect bytes of
//! their choosing. So the test that matters most here is not that a good
//! handshake echoes — it is that a bad one does not, and that the response
//! carries no trace of the value it was asked to reflect.
//!
//! # Acknowledged is not the same as acted on
//!
//! Past the signature this route answers 200 to everything, because a 4xx would
//! put a delivery nobody can act on into a provider's retry loop. That is only
//! correct while nothing IS acted on, and a status code cannot show it — so
//! every acknowledgement here is checked against the event rows too.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_connector::Provider;
use afd_core::error_code;
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::{json_body, send_with_headers};
#[path = "connector_events_live/fixture.rs"]
mod fixture;

use self::fixture::{Configured, Fixture, SIGNING_SECRET, WRONG_SECRET};

/// The scheme this provider's deliveries are signed under.
///
/// Read from the registry rather than named, so a scheme change moves the
/// fixture with the daemon instead of leaving it signing the old way and
/// reading the refusal as a verdict (RULE TFX).
const SCHEME: Scheme = Scheme::SlackV0;

/// The provider these deliveries arrive for.
const PROVIDER: Provider = Provider::Slack;

/// The value a handshake asks this daemon to reflect.
const CHALLENGE: &str = "3eZbrw1aB1CaQdLQCbtx";

/// The field it is carried and echoed under.
const FIELD_CHALLENGE: &str = "challenge";

/// A subscription handshake, as a provider opens one with.
const HANDSHAKE: &str = r#"{"type":"url_verification","challenge":"3eZbrw1aB1CaQdLQCbtx"}"#;

/// A real delivery the route understands and deliberately does not act on: an
/// event callback that is not a mention of the bot.
const MESSAGE: &str = r#"{"type":"event_callback","team_id":"T024BE7LD","event_id":"Ev0NOTMENTION","event":{"type":"reaction_added","user":"U01"}}"#;

/// The reason a signed event the route does not act on is dropped.
const REASON_UNSUPPORTED: &str = "unsupported_event";

/// The field a dropped delivery names its reason in.
const FIELD_IGNORED: &str = "ignored";

/// Where this provider's deliveries arrive.
fn path() -> String {
    format!("/v1/connectors/{}/events", PROVIDER.id())
}

/// One delivery of `body`, signed with `secret` at the router's own instant.
///
/// Frozen rather than the wall clock: `SlackV0` binds a timestamp and is
/// checked against `services.now()`, so a fixture signing at `SystemTime::now`
/// would be sixty-odd million seconds adrift and every case here would read as
/// stale rather than as what it meant to prove.
async fn deliver(router: &axum::Router, secret: &[u8], body: &str) -> axum::response::Response {
    let at = harness::frozen_unix_seconds().to_string();
    let proof = harness::webhook::signature_at(SCHEME, secret, Some(&at), body.as_bytes());
    let headers = harness::webhook::slack_headers(&proof, &at);
    send_with_headers(router, Method::POST, &path(), None, body, &headers).await
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_signed_handshake_is_echoed_from_the_secret_the_vault_holds() {
    let fixture = Fixture::create().await;
    fixture.seed(Configured::Signing).await;
    let router = fixture.router();

    let answered = deliver(&router, SIGNING_SECRET, HANDSHAKE).await;
    let status = answered.status();
    let document = json_body(answered).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get(FIELD_CHALLENGE).and_then(Value::as_str),
        Some(CHALLENGE),
        "the echo is what proves the endpoint to the provider, and it is only \
         reachable through a secret that was opened out of the vault"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_handshake_signed_with_the_wrong_secret_reflects_nothing() {
    // The reflection case, and the reason the echo lives behind the wall. An
    // unverified echo would confirm this path to anyone who guessed it and turn
    // the endpoint into a reflector for bytes of a caller's choosing.
    let fixture = Fixture::create().await;
    fixture.seed(Configured::Signing).await;
    let router = fixture.router();

    let refused = deliver(&router, WRONG_SECRET, HANDSHAKE).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_SIGNATURE_INVALID.as_str())
    );
    assert!(
        !document.to_string().contains(CHALLENGE),
        "a refusal that echoed the challenge anywhere in its body would be the \
         reflection this ordering exists to prevent: {document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_connector_configured_without_a_signing_secret_verifies_nothing() {
    // A bag that OPENS and carries no signing secret, which is a different
    // state from holding no bag: a deployment connected the provider and never
    // configured its inbound half. Both answer UZ-WH-020, and the reason is
    // fail-closed — with no secret there is nothing to check a signature
    // against, and accepting unverified deliveries on a public endpoint is
    // worse than serving none.
    let fixture = Fixture::create().await;
    fixture.seed(Configured::WithoutSecret).await;
    let router = fixture.router();

    let refused = deliver(&router, SIGNING_SECRET, HANDSHAKE).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED.as_str()),
        "a bag with no signing secret is unconfigured, not a bad signature"
    );
    assert!(!document.to_string().contains(CHALLENGE));

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_signed_event_the_route_does_not_act_on_is_acknowledged_and_acted_on_by_nothing() {
    // The fast-ack rule, and the half of it a status code cannot show. A
    // provider retries anything that is not a 2xx and disables an endpoint that
    // keeps failing, so a real delivery this route does not act on — anything
    // but a mention of the bot — is acknowledged with its reason. That is only correct while nothing acted on
    // it, which is what the event count is here for.
    let fixture = Fixture::create().await;
    fixture.seed(Configured::Signing).await;
    let router = fixture.router();
    let before = fixture.fleet_events().await;

    let acknowledged = deliver(&router, SIGNING_SECRET, MESSAGE).await;
    let status = acknowledged.status();
    let document = json_body(acknowledged).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a 4xx here would retry-loop a delivery that will parse no better the \
         second time: {document}"
    );
    assert_eq!(
        document.get(FIELD_IGNORED).and_then(Value::as_str),
        Some(REASON_UNSUPPORTED),
        "the reason is the answer, because the sender is an app that will never \
         read it and an operator asking why nothing happened has only this"
    );
    assert_eq!(
        fixture.fleet_events().await,
        before,
        "acknowledged is not acted on"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_retried_delivery_is_answered_identically_and_still_acts_on_nothing() {
    // A provider's at-least-once delivery, which is the ordinary case rather
    // than an adversarial one. For an event the route does not act on,
    // idempotence is held because nothing is written; a mention's retry is
    // held by the admission ledger, which `integration_slack_mention` proves.
    let fixture = Fixture::create().await;
    fixture.seed(Configured::Signing).await;
    let router = fixture.router();
    let before = fixture.fleet_events().await;

    let first = json_body(deliver(&router, SIGNING_SECRET, MESSAGE).await).await;
    let retried = json_body(deliver(&router, SIGNING_SECRET, MESSAGE).await).await;
    assert_eq!(
        first, retried,
        "a retry earns the answer the first delivery did, so a provider's \
         redelivery policy cannot change what this endpoint reports"
    );
    assert_eq!(
        fixture.fleet_events().await,
        before,
        "two deliveries, and still nothing acted on either"
    );

    fixture.cleanup().await;
}
