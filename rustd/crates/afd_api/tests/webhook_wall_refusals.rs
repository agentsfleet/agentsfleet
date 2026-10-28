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

use crate::harness;

use std::sync::Arc;

use self::harness::webhook as signed;
use self::harness::{Fleet, Scripted, refusal_code, send_with_headers};
use afd_core::error_code;
use afd_fleet_lifecycle::FleetStatus;
use afd_webhook::Scheme;
use http::{Method, StatusCode};

/// A failed run — a delivery that WOULD wake the fleet if it proved itself.
const RUN_FAILURE: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure.json");

/// The event kind the fixture is.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

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
    case: &str,
) -> String {
    let router = Fleet::new().with_ingress(ingress).router();
    let response = send_with_headers(&router, Method::POST, &path(), None, body, headers).await;

    let code = refusal_code(response).await;
    assert!(
        ingress.deliveries().is_empty(),
        "{case}: a refused delivery must reach no stream — a route that appended \
         first would answer this same code with the fleet already woken"
    );
    code
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

/// Every way a delivery can fail to prove itself earns one answer.
///
/// An absent proof, a wrong key, a tampered body and a digest missing its
/// scheme prefix all answer `UZ-WH-010`: telling them apart narrows a forger's
/// search for no honest sender's benefit.
#[tokio::test]
async fn a_delivery_that_does_not_prove_itself_is_refused_and_appends_nothing() {
    let honest = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let wrong_key = signed::signature(
        Scheme::BodyHex,
        signed::WRONG_SECRET,
        RUN_FAILURE.as_bytes(),
    );
    // The right length of hex with the scheme's prefix missing — the shape a
    // sender lands on by stripping the prefix themselves.
    let unprefixed = honest.trim_start_matches(Scheme::BodyHex.prefix());
    // Signed over the fixture, sent with one byte more. The tag covers the
    // BODY, so this is the case that would pass if a route ever re-serialized
    // a parsed payload before hashing it.
    let tampered = format!("{RUN_FAILURE} ");
    let cases: [(&str, Option<&str>, &str); 4] = [
        ("no signature at all", None, RUN_FAILURE),
        (
            "a signature from the wrong key",
            Some(&wrong_key),
            RUN_FAILURE,
        ),
        ("a body changed after signing", Some(&honest), &tampered),
        (
            "a digest without its scheme prefix",
            Some(unprefixed),
            RUN_FAILURE,
        ),
    ];

    for (case, proof, body) in cases {
        let ingress = holding(signed::TRIGGER_GITHUB, Some(signed::SECRET));
        let headers = match proof {
            Some(proof) => signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, proof),
            None => vec![
                (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
                (signed::name(signed::HEADER_DELIVERY), signed::DELIVERY_ID),
            ],
        };
        assert_eq!(
            refused(&ingress, &headers, body, case).await,
            error_code::WEBHOOK_SIGNATURE_INVALID.as_str(),
            "{case}"
        );
    }
}

#[tokio::test]
async fn a_fleet_with_no_stored_secret_is_refused_before_any_verify_is_attempted() {
    let ingress = holding(signed::TRIGGER_GITHUB, None);
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());
    let headers = signed::github_headers(EVENT_WORKFLOW_RUN, signed::DELIVERY_ID, &proof);

    assert_eq!(
        refused(&ingress, &headers, RUN_FAILURE, "no stored secret").await,
        error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED.as_str(),
        "a valid signature over a fleet with no secret is still unconfigured — \
         this daemon has nothing to have checked it against"
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
        refused(&ingress, &headers, RUN_FAILURE, "no fleet row").await,
        error_code::WEBHOOK_FLEET_NOT_FOUND.as_str()
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
        refused(&ingress, &headers, RUN_FAILURE, "no event header").await,
        error_code::WEBHOOK_MALFORMED.as_str(),
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
