//! What the signature wall refuses, and — the load-bearing half — in what order.
//!
//! # The assertion that matters is what did NOT happen
//!
//! Every case here checks two things: the code the sender was answered with,
//! and that the ingress recorded no append. The second is the security
//! property. A route that verified correctly but appended first would answer
//! the same `UZ-WH-010` while having already woken the fleet, and a test
//! reading only the status code would call that a pass.
//!
//! # Why an unconfigured secret is not a signature failure
//!
//! `UZ-WH-020` and `UZ-WH-010` are different answers to different questions:
//! one says this daemon has nothing to check against, the other says the check
//! ran and failed. Collapsing them would send an operator whose vault entry is
//! missing to look for a key-rotation skew.
//!
//! # Why a missing fleet and a fleet with no trigger are ONE answer
//!
//! Both are `UZ-WH-001`. Telling them apart confirms a fleet id to whoever
//! guessed it, which is the whole of what an unauthenticated prober wants from
//! this endpoint.

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
use afd_fleet_lifecycle::FleetStatus;
use afd_webhook::Scheme;
use http::{Method, StatusCode};

/// A failed run — a delivery that WOULD wake the fleet if it proved itself.
const RUN_FAILURE: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure.json");

/// The event kind the fixture is.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// A trigger naming a provider this daemon ships no signature scheme for.
///
/// `zoho` is a connector provider rather than a webhook one, so
/// [`Scheme::for_source`] answers `None` for it — which is the fail-closed
/// branch, not a pass.
const TRIGGER_UNKNOWN_SCHEME: &str = r#"[{"type":"webhook","source":"zoho"}]"#;

/// Where one fleet's GitHub deliveries arrive.
fn path() -> String {
    format!("/v1/webhooks/{}/github", signed::FLEET)
}

/// The refusal a request earned, and the appends it did not cause.
///
/// Both halves in one helper so no case can assert the code and forget the
/// silence — see the module note on why the second is the real property.
async fn refused(
    ingress: &Arc<Scripted>,
    headers: &[(http::HeaderName, &str)],
    body: &str,
) -> String {
    let router = Fleet::new().with_ingress(ingress).router();
    let response = send_with_headers(&router, Method::POST, &path(), None, body, headers).await;

    let status = response.status();
    let document = json_body(response).await;
    assert!(
        status.is_client_error(),
        "a refusal is the sender's to fix, so it is a 4xx: {status} {document}"
    );
    assert!(
        ingress.deliveries().is_empty(),
        "a refused delivery must reach no stream — a route that appended first \
         would answer this same code with the fleet already woken"
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

/// An ingress resolving this fleet, holding `secret`.
fn holding(triggers: &str, secret: Option<&[u8]>) -> Arc<Scripted> {
    let scripted = Scripted::new().resolving(signed::binding_with_status(
        triggers,
        FleetStatus::Active.as_str(),
    ));
    Arc::new(match secret {
        Some(bytes) => scripted.signing(bytes),
        None => scripted,
    })
}

#[tokio::test]
async fn a_delivery_carrying_no_signature_is_refused_and_appends_nothing() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    let headers = vec![
        (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
        (signed::name(signed::HEADER_DELIVERY), signed::DELIVERY_ID),
    ];

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "an absent proof and a wrong proof are one answer: telling them apart \
         narrows a forger's search for no honest sender's benefit"
    );
}

#[tokio::test]
async fn a_signature_from_the_wrong_key_is_refused_and_appends_nothing() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    let forged = signed::signature(
        Scheme::BodyHex,
        signed::WRONG_SECRET,
        RUN_FAILURE.as_bytes(),
    );
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &forged);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_body_changed_after_signing_is_refused_and_appends_nothing() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    // Signed over the fixture, sent with one byte more. The tag covers the
    // BODY, so this is the case that would pass if a route ever re-serialized
    // a parsed payload before hashing it.
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);
    let tampered = format!("{RUN_FAILURE} ");

    assert_eq!(
        refused(&ingress, &headers, &tampered).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID)
    );
}

#[tokio::test]
async fn a_malformed_signature_header_is_refused_and_appends_nothing() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    // The right length of hex with the scheme's prefix missing — the shape a
    // sender lands on by stripping the prefix themselves.
    let bare = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes())
        .trim_start_matches(Scheme::BodyHex.prefix())
        .to_owned();
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &bare);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_SIGNATURE_INVALID),
        "the prefix is part of the scheme, so a digest without it is not a proof"
    );
}

#[tokio::test]
async fn a_fleet_with_no_stored_secret_is_refused_before_any_verify_is_attempted() {
    let ingress = holding(signed::TRIGGER_GITHUB, None);
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "a valid signature over a fleet with no secret is still unconfigured — \
         this daemon has nothing to have checked it against"
    );
}

#[tokio::test]
async fn a_source_this_daemon_ships_no_scheme_for_fails_closed_rather_than_open() {
    let ingress = holding(TRIGGER_UNKNOWN_SCHEME, Some(signed::SECRET));
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED),
        "an unrecognised provider is refused, never passed — and refused BEFORE \
         the vault is asked, so a probe cannot measure the decrypt"
    );
}

#[tokio::test]
async fn a_fleet_this_daemon_serves_no_row_for_is_refused_as_not_found() {
    // Resolving nothing is both "no such fleet" and "a fleet declaring no
    // webhook trigger" — see the module note on why they are one answer.
    let ingress = Arc::new(Scripted::new());
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_FLEET_NOT_FOUND)
    );
}

#[tokio::test]
async fn a_delivery_naming_no_event_is_refused_before_the_wall_is_reached() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = vec![
        (signed::name(signed::HEADER_DELIVERY), signed::DELIVERY_ID),
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
    ];

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE).await,
        code(error_code::WEBHOOK_MALFORMED),
        "the event kind is read from the header before anything else, so a \
         delivery that names none is refused without a vault read"
    );
}

#[tokio::test]
async fn a_fleet_id_that_is_not_an_identifier_is_refused_without_a_lookup() {
    let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
    let router = Fleet::new().with_ingress(&ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);

    let response = send_with_headers(
        &router,
        Method::POST,
        "/v1/webhooks/not-an-identifier/github",
        None,
        RUN_FAILURE,
        &headers,
    )
    .await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert!(
        ingress.deliveries().is_empty(),
        "a path that cannot name a fleet reaches no store"
    );
}
