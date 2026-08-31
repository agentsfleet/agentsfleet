//! `POST /v1/webhooks/{fleet_id}` — one fleet's deliveries, any provider.
//!
//! The provider-agnostic sibling of `/v1/webhooks/{fleet_id}/github`, and until
//! now the untested one: `webhook_fleet_route.rs` addresses the GitHub-flavoured
//! path, which is a different handler that reads an event kind out of a header.
//! This route knows only which fleet the URL named — the provider, and the
//! scheme its signature is checked under, come from the fleet's own trigger
//! rather than from anything the sender said.
//!
//! # What that difference means for the tests
//!
//! There is no classification here and no allow-list, so a verified delivery is
//! either appended or the fleet is not runnable. The interesting cases are
//! therefore about WHAT is appended — the identifier a redelivery has to
//! repeat, and the actor a fleet's history records — rather than about which
//! deliveries survive a policy.
//!
//! # No datastore
//!
//! Every decision past the wall is a function of the binding the ingress seam
//! answered, so the whole matrix runs with the store scripted and no Postgres
//! or Redis anywhere.

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
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A delivery body, which on this surface need only BE a JSON document.
///
/// The fleet's own prose is what reads it, so the daemon checks that it parses
/// and hands the bytes on unchanged — there is no schema to check it against.
const PAYLOAD: &str = r#"{"kind":"deploy.finished","ref":"refs/heads/main"}"#;

/// A body that is not a document at all.
const NOT_JSON: &str = "deploy.finished refs/heads/main";

/// What a fleet's history records as having woken it.
///
/// The provider, never the person whose push produced the event: recording a
/// sender's login would let an actor-shaped assertion certify that a human woke
/// this fleet when a webhook did.
const ACTOR_GITHUB: &str = "webhook:github";

/// The reason a delivery to a fleet that is not runnable is dropped.
const REASON_PAUSED: &str = "fleet_paused";

/// A fleet id that is not one.
const UNPARSEABLE_FLEET: &str = "not-a-uuid";

/// Where one fleet's deliveries arrive.
fn path(fleet: &str) -> String {
    format!("/v1/webhooks/{fleet}")
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A fleet resolving to `status`, holding the fixture signing secret.
fn fleet_in(status: &str) -> Arc<Scripted> {
    Arc::new(
        Scripted::new()
            .resolving(signed::binding_with_status(signed::TRIGGER_GITHUB, status))
            .signing(signed::SECRET),
    )
}

/// One delivery of `body` to `fleet`, signed with `secret`.
async fn deliver(
    ingress: &Arc<Scripted>,
    fleet: &str,
    secret: &[u8],
    body: &str,
) -> axum::response::Response {
    let router = Fleet::new().with_ingress(ingress).router();
    let proof = signed::signature(Scheme::BodyHex, secret, body.as_bytes());
    let headers = vec![(
        signed::name(Scheme::BodyHex.signature_header()),
        proof.as_str(),
    )];
    send_with_headers(&router, Method::POST, &path(fleet), None, body, &headers).await
}

#[tokio::test]
async fn a_signed_delivery_wakes_the_fleet_and_answers_the_events_id() {
    let ingress = fleet_in("active");
    let woken = deliver(&ingress, signed::FLEET, signed::SECRET, PAYLOAD).await;

    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(false),
        "the first delivery is the one that wrote it"
    );
    assert!(
        document
            .get("event_id")
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty()),
        "the sender is answered the id its redelivery will repeat: {document}"
    );

    let appends = ingress.deliveries();
    assert_eq!(appends.len(), 1);
    let append = appends.first().expect("the delivery was appended");
    assert_eq!(
        append.surface,
        Surface::Fleet,
        "the per-fleet claim window, not the App's — they expire differently"
    );
    assert_eq!(append.actor, ACTOR_GITHUB);
    assert_eq!(
        append.request_json, PAYLOAD,
        "the bytes a fleet reasons over are the bytes the sender signed, \
         unchanged — a re-serialized document would no longer verify"
    );
}

#[tokio::test]
async fn a_redelivery_repeats_the_first_claim_and_reports_that_it_did() {
    // A provider retries on its own schedule for as long as its policy says,
    // and nothing downstream will ever tell us the delivery is settled. The
    // second attempt must answer the FIRST attempt's id, or a sender
    // reconciling its delivery log sees two events for one thing that happened.
    let ingress = fleet_in("active");
    let first = deliver(&ingress, signed::FLEET, signed::SECRET, PAYLOAD).await;
    let first = json_body(first).await;

    let again = deliver(&ingress, signed::FLEET, signed::SECRET, PAYLOAD).await;
    let status = again.status();
    let again = json_body(again).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{again}");
    assert_eq!(
        again.get("event_id"),
        first.get("event_id"),
        "the redelivery is answered the first attempt's id"
    );
    assert_eq!(
        again.get("replayed").and_then(Value::as_bool),
        Some(true),
        "and told that is what happened"
    );
}

#[tokio::test]
async fn a_delivery_to_a_fleet_that_is_not_runnable_is_acknowledged_and_dropped() {
    // Acknowledged rather than refused: the sender configured this URL against
    // a fleet somebody has since paused, and a 4xx would retry-loop a delivery
    // that will be just as paused next time.
    let ingress = fleet_in("paused");
    let answered = deliver(&ingress, signed::FLEET, signed::SECRET, PAYLOAD).await;

    let status = answered.status();
    let document = json_body(answered).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(REASON_PAUSED)
    );
    assert!(
        ingress.deliveries().is_empty(),
        "a paused fleet must not accumulate work to run when it resumes"
    );
}

#[tokio::test]
async fn a_verified_body_that_is_not_a_document_is_refused() {
    // The one refusal past the wall. A fleet's prose reasons over a document,
    // and cannot reason over a form-encoded string or a fragment of XML — so
    // this is the sender's to fix, unlike everything else here.
    let ingress = fleet_in("active");
    let refused = deliver(&ingress, signed::FLEET, signed::SECRET, NOT_JSON).await;

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
async fn a_signature_under_the_wrong_key_wakes_nothing() {
    let ingress = fleet_in("active");
    let refused = deliver(&ingress, signed::FLEET, signed::WRONG_SECRET, PAYLOAD).await;

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
async fn a_path_naming_no_canonical_fleet_is_refused_before_the_wall() {
    // Before the signature, and that ordering is deliberate: an identifier that
    // is not one cannot name a fleet whose secret could be read, so verifying
    // first would spend a vault read on a request that cannot succeed.
    let ingress = fleet_in("active");
    let refused = deliver(&ingress, UNPARSEABLE_FLEET, signed::SECRET, PAYLOAD).await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert!(status.is_client_error(), "{status} {document}");
    assert!(ingress.deliveries().is_empty());
}
