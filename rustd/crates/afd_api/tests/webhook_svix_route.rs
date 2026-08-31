//! `POST /v1/webhooks/svix/{fleet_id}` — deliveries proven by a vendored verifier.
//!
//! The third way a delivery reaches one fleet, and the only one whose signature
//! this daemon does not check with its own canon: Svix publishes a scheme, and
//! `afd_webhook::vendor::svix` implements exactly that so a sender configured
//! against any Svix-fronted provider verifies here without per-provider code.
//!
//! # The delivery id comes from the SENDER, and that is the whole difference
//!
//! Every other ingress route derives its claim from the body — a content hash,
//! or the scheduler's own message id. Svix hands one over in a header, and this
//! route uses it, because a sender that retries a delivery reuses that id and a
//! content hash would suppress a genuinely new delivery whose body happened to
//! match.
//!
//! The route has no fallback for an absent header, and cannot have one: the id
//! it claims on comes back FROM the verifier, so the only value that reaches a
//! delivery is the one `svix::verify_at` already proved non-empty and signed.
//! `a_delivery_carrying_no_identifier_is_refused_by_the_wall` is what holds
//! that shut — a fallback reintroduced here would put every unidentified
//! delivery on one shared per-fleet slot, and only the status code would say
//! so.

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
use afd_ingress::Surface;
use http::{Method, StatusCode};
use serde_json::Value;

/// A delivery body, which on this surface need only BE a JSON document.
const PAYLOAD: &str = r#"{"type":"invoice.paid","data":{"id":"in_1QfixtureA"}}"#;

/// A second delivery, so a claim keyed on the sender's id can be told apart
/// from one keyed on the body.
const OTHER_PAYLOAD: &str = r#"{"type":"invoice.paid","data":{"id":"in_1QfixtureB"}}"#;

/// A body that is not a document at all.
const NOT_JSON: &str = "invoice.paid in_1QfixtureA";

/// What a fleet's history records as having woken it.
const ACTOR_GITHUB: &str = "webhook:github";

/// The reason a delivery to a fleet that is not runnable is dropped.
const REASON_PAUSED: &str = "fleet_paused";

/// A second delivery identifier, for the retry-versus-new-delivery case.
const OTHER_DELIVERY: &str = "msg_2fJk8Lq0PsWzXbYtRnVdEcHgMb";

/// The instant this fixture signs and verifies at — frozen, not the wall clock.
///
/// Svix binds a timestamp into its signed bytes and checks it against
/// `services.now()`, so a fixture signing at `SystemTime::now` would be
/// sixty-odd million seconds adrift and every case here would read as stale.
fn now() -> i64 {
    harness::frozen_unix_seconds()
}

/// Where a Svix-fronted delivery for one fleet arrives.
fn path(fleet: &str) -> String {
    format!("/v1/webhooks/svix/{fleet}")
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A fleet resolving to `status`, whose workspace holds the Svix secret.
fn fleet_in(status: &str) -> Arc<Scripted> {
    Arc::new(
        Scripted::new()
            .resolving(signed::binding_with_status(signed::TRIGGER_GITHUB, status))
            .svix_signing(signed::SVIX_SECRET),
    )
}

/// One delivery of `body`, presented under `delivery` and signed correctly.
async fn deliver(ingress: &Arc<Scripted>, delivery: &str, body: &str) -> axum::response::Response {
    let at = now();
    let proof = signed::svix_signature(delivery, at, body);
    send_signed(ingress, delivery, body, &proof).await
}

/// The same, carrying `proof` verbatim so a case can present a bad one.
async fn send_signed(
    ingress: &Arc<Scripted>,
    delivery: &str,
    body: &str,
    proof: &str,
) -> axum::response::Response {
    let router = Fleet::new().with_ingress(ingress).router();
    let stamp = now().to_string();
    let headers = signed::svix_headers(delivery, &stamp, proof);
    let target = path(signed::FLEET);
    send_with_headers(&router, Method::POST, &target, None, body, &headers).await
}

#[tokio::test]
async fn a_signed_delivery_wakes_the_fleet_under_the_senders_own_identifier() {
    let ingress = fleet_in("active");
    let woken = deliver(&ingress, signed::SVIX_DELIVERY, PAYLOAD).await;

    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(false)
    );

    let appends = ingress.deliveries();
    assert_eq!(appends.len(), 1);
    let append = appends.first().expect("the delivery was appended");
    assert_eq!(append.surface, Surface::Fleet);
    assert_eq!(append.actor, ACTOR_GITHUB);
    assert_eq!(
        append.event_id,
        signed::SVIX_DELIVERY,
        "the claim is the SENDER's identifier, which is what its retry repeats"
    );
    assert_eq!(append.request_json, PAYLOAD);
}

#[tokio::test]
async fn a_retry_of_one_delivery_is_suppressed_and_a_new_one_is_not() {
    // Both halves matter and they pull in opposite directions. Keying on the
    // sender's id suppresses its retry; keying on the body would ALSO suppress
    // a genuinely new delivery that happened to carry identical bytes, which on
    // a surface like invoicing is a real event rather than a contrived one.
    let ingress = fleet_in("active");
    assert_eq!(
        deliver(&ingress, signed::SVIX_DELIVERY, PAYLOAD)
            .await
            .status(),
        StatusCode::ACCEPTED
    );

    let retried = deliver(&ingress, signed::SVIX_DELIVERY, PAYLOAD).await;
    let retried = json_body(retried).await;
    assert_eq!(
        retried.get("replayed").and_then(Value::as_bool),
        Some(true),
        "the same delivery id is the same delivery"
    );

    let fresh = deliver(&ingress, OTHER_DELIVERY, OTHER_PAYLOAD).await;
    let fresh = json_body(fresh).await;
    assert_eq!(
        fresh.get("replayed").and_then(Value::as_bool),
        Some(false),
        "a different delivery id is a different delivery"
    );
    assert_eq!(
        ingress.deliveries().len(),
        3,
        "every attempt reached the store"
    );
}

#[tokio::test]
async fn a_delivery_carrying_no_identifier_is_refused_by_the_wall() {
    // An unidentified delivery earns a refusal, never a narrower claim: the
    // route has no id of its own to fall back to, because `verified_svix`
    // hands the claim key back only on the path where `svix::verify_at`
    // already refused an empty `svix-id`. This test is what holds that shut —
    // a fallback reintroduced here would put every unidentified delivery on
    // one shared slot, and only the status code would say so.
    let ingress = fleet_in("active");
    let router = Fleet::new().with_ingress(&ingress).router();
    let at = now();
    let proof = signed::svix_signature("", at, PAYLOAD);
    let stamp = at.to_string();
    let headers = vec![
        (
            signed::name(afd_webhook::vendor::svix::TIMESTAMP_HEADER),
            stamp.as_str(),
        ),
        (
            signed::name(afd_webhook::vendor::svix::SIGNATURE_HEADER),
            proof.as_str(),
        ),
    ];
    let target = path(signed::FLEET);
    let refused = send_with_headers(&router, Method::POST, &target, None, PAYLOAD, &headers).await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_SIGNATURE_INVALID))
    );
    assert!(
        ingress.deliveries().is_empty(),
        "the wall refuses before a claim key exists, so nothing is appended \
         under any identifier"
    );
}

#[tokio::test]
async fn a_delivery_to_a_fleet_that_is_not_runnable_is_acknowledged_and_dropped() {
    let ingress = fleet_in("paused");
    let answered = deliver(&ingress, signed::SVIX_DELIVERY, PAYLOAD).await;

    let status = answered.status();
    let document = json_body(answered).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(REASON_PAUSED)
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_verified_body_that_is_not_a_document_is_refused() {
    let ingress = fleet_in("active");
    let refused = deliver(&ingress, signed::SVIX_DELIVERY, NOT_JSON).await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert!(status.is_client_error(), "{status} {document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_MALFORMED))
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_signature_bound_to_another_delivery_is_refused() {
    // The binding is the point of the scheme: the signed bytes are
    // `id.timestamp.body`, so a proof lifted from one delivery onto another
    // fails even though both are genuine and both were signed by the sender.
    let ingress = fleet_in("active");
    let lifted = signed::svix_signature(OTHER_DELIVERY, now(), PAYLOAD);
    let refused = send_signed(&ingress, signed::SVIX_DELIVERY, PAYLOAD, &lifted).await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_SIGNATURE_INVALID))
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_fleet_whose_workspace_holds_no_svix_secret_verifies_nothing() {
    // Fail-closed, and not a degradation: with no secret there is nothing to
    // check a signature against, and accepting an unverified delivery on a
    // public endpoint is strictly worse than serving none.
    let ingress = Arc::new(Scripted::new().resolving(signed::binding(signed::TRIGGER_GITHUB)));
    let refused = deliver(&ingress, signed::SVIX_DELIVERY, PAYLOAD).await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED))
    );
    assert!(ingress.deliveries().is_empty());
}
