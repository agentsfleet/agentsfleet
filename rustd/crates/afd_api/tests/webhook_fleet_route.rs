//! What `POST /v1/webhooks/{fleet_id}/github` accepts, and what it drops.
//!
//! # Why these run through the router rather than against the handler
//!
//! The order this route decides in is the thing worth proving, and half of it
//! is decided by the layers in front of the handler — the path parse, the body
//! buffer, the envelope the refusal is rendered into. A test calling `receive`
//! directly would see none of that and would still pass on the day a layer
//! started reading the body first.
//!
//! # Every drop here is a 200, and that is the assertion
//!
//! A green build, an event outside the allow-list, a paused fleet: real
//! deliveries, correctly signed, none of them waking anything. GitHub retries
//! non-2xx for three days and retrying changes none of them, so the reason
//! travels in the BODY and the status stays 200. A test that only checked "not
//! an error" would pass for a 500.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::harness;

use std::sync::Arc;

use self::harness::webhook as signed;
use self::harness::{Fleet, Recorded, Scripted, json_body, send_with_headers};
use afd_core::error_code;
use afd_fleet_lifecycle::FleetStatus;
use afd_ingress::Surface;
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A failed run — the delivery that wakes a fleet.
const RUN_FAILURE: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure.json");

/// The same run, green — a real delivery this daemon deliberately drops.
const RUN_SUCCESS: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_success.json");

/// The event kind both run fixtures are.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// An event kind no fixture is, for the allow-list cases.
const EVENT_PUSH: &str = "push";

/// A trigger admitting pushes and nothing else.
const TRIGGER_PUSH_ONLY: &str = r#"[{"type":"webhook","source":"github","events":["push"]}]"#;

/// The actor a GitHub-driven wake records — `github_route.rs`'s `ACTOR_GITHUB`.
const ACTOR_GITHUB: &str = "webhook:github";

/// The reason the route drops a delivery outside the trigger's allow-list.
const REASON_EVENT_NOT_SUBSCRIBED: &str = "event_not_subscribed";

/// The reason the route drops a delivery to a fleet nobody is running.
const REASON_FLEET_PAUSED: &str = "fleet_paused";

/// The reason `classify` gives for a run that did not fail.
const REASON_NON_FAILURE_CONCLUSION: &str = "non_failure_conclusion";

/// Where one fleet's GitHub deliveries arrive.
fn path() -> String {
    format!("/v1/webhooks/{}/github", signed::FLEET)
}

/// An ingress that resolves this fleet and holds its secret.
fn serving(triggers: &str, status: &str) -> Arc<Scripted> {
    Arc::new(
        Scripted::new()
            .resolving(signed::binding_with_status(triggers, status))
            .signing(signed::SECRET),
    )
}

/// One signed delivery of `body`, presented as `event`.
async fn deliver(ingress: &Arc<Scripted>, event: &str, delivery: &str, body: &str) -> Response {
    let router = Fleet::new().with_ingress(ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, body.as_bytes());
    let headers = signed::github_headers(event, delivery, &proof);
    send_with_headers(&router, Method::POST, &path(), None, body, &headers).await
}

/// What one request answered.
type Response = axum::response::Response;

/// One field of a response document, or `Null` where it is absent.
///
/// Absent reads as `Null` rather than panicking so an assertion failure names
/// the field that was missing instead of the line that looked for it.
fn field<'d>(document: &'d Value, name: &str) -> &'d Value {
    document.get(name).unwrap_or(&Value::Null)
}

/// The `ignored` reason a 200 carries.
async fn ignored_reason(response: Response) -> String {
    let status = response.status();
    let document = json_body(response).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "a dropped delivery answers 200: {document}"
    );
    field(&document, "ignored")
        .as_str()
        .expect("a dropped delivery names the rule that dropped it")
        .to_owned()
}

#[tokio::test]
async fn a_signed_failed_run_wakes_the_fleet_and_answers_the_events_id() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let response = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let document = json_body(response).await;
    assert_eq!(
        *field(&document, "replayed"),
        Value::Bool(false),
        "the first delivery of an event is not a replay"
    );

    let appended = ingress.deliveries();
    assert_eq!(
        appended.len(),
        1,
        "one delivery is one append: {appended:?}"
    );
    let Recorded {
        surface,
        fleet,
        event_id,
        actor,
        ..
    } = appended.first().expect("one delivery is one append");

    assert_eq!(
        *surface,
        Surface::Fleet,
        "the URL named the fleet, so the claim is the per-fleet one"
    );
    assert_eq!(fleet, signed::FLEET);
    assert_eq!(
        event_id,
        &afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
        "the claim key is the digest of the body the SIGNATURE covered, never \
         the delivery header — GitHub signs the body alone, so a header key is \
         one the resender chooses"
    );
    assert_eq!(
        actor, ACTOR_GITHUB,
        "a webhook wake names the provider and no person"
    );
    assert!(
        field(&document, "event_id")
            .as_str()
            .is_some_and(|id| !id.is_empty()),
        "the response carries the stream id an operator searches history by: \
         {document}"
    );
}

#[tokio::test]
async fn a_redelivery_repeats_the_first_claim_and_reports_that_it_did() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;
    let first = json_body(first).await;

    let second = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);
    let second = json_body(second).await;

    assert_eq!(
        field(&second, "event_id"),
        field(&first, "event_id"),
        "a sender comparing two responses must see the same event both times"
    );
    assert_eq!(
        *field(&second, "replayed"),
        Value::Bool(true),
        "a repeat is REPORTED rather than hidden — a lost delivery and a \
         suppressed one are different facts to whoever is debugging"
    );

    let keys: Vec<String> = ingress
        .deliveries()
        .into_iter()
        .map(|recorded| recorded.event_id)
        .collect();
    assert_eq!(
        keys,
        [
            afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
            afd_ingress::replay_id(RUN_FAILURE.as_bytes())
        ],
        "both attempts claim under the SAME key; a key that varied per attempt \
         would make every retry a fresh event and run the fleet twice"
    );
}

#[tokio::test]
async fn an_event_outside_the_allow_list_is_dropped_before_the_body_is_read() {
    let ingress = serving(TRIGGER_PUSH_ONLY, FleetStatus::Active.as_str());

    // A `workflow_run` BODY presented under a trigger that admits only pushes.
    let response = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;

    assert_eq!(ignored_reason(response).await, REASON_EVENT_NOT_SUBSCRIBED);
    assert!(
        ingress.deliveries().is_empty(),
        "an unsubscribed event must not reach the stream"
    );
}

#[tokio::test]
async fn an_allow_list_is_measured_against_the_header_not_the_payload() {
    let ingress = serving(TRIGGER_PUSH_ONLY, FleetStatus::Active.as_str());

    // The header says `push`, which the allow-list admits, while the body is a
    // `workflow_run`. The delivery gets past the allow-list on the header's
    // word and is then refused by the parse — which is the order that matters:
    // an author's allow-list is written in GitHub's event vocabulary, and
    // inferring the kind from the payload would let a holder of the secret
    // present a body that classifies as one event while the header says another.
    let response = deliver(&ingress, EVENT_PUSH, signed::DELIVERY_ID, RUN_FAILURE).await;

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let document = json_body(response).await;
    assert_eq!(
        *field(&document, "error_code"),
        Value::String(error_code::WEBHOOK_MALFORMED.as_str().to_owned()),
        "a body that is not the event its own header claims is the sender's bug"
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_paused_fleet_answers_two_hundred_rather_than_earning_a_retry() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Paused.as_str());

    let response = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;

    assert_eq!(ignored_reason(response).await, REASON_FLEET_PAUSED);
    assert!(
        ingress.deliveries().is_empty(),
        "a fleet somebody paused on purpose takes no work"
    );
}

#[tokio::test]
async fn a_green_run_is_dropped_with_its_reason_rather_than_waking_the_fleet() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let response = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_SUCCESS,
    )
    .await;

    assert_eq!(
        ignored_reason(response).await,
        REASON_NON_FAILURE_CONCLUSION,
        "the classifier's reason reaches the sender rather than a bare 200"
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_delivery_with_no_identifier_still_gets_a_claim_key() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());
    let router = Fleet::new().with_ingress(&ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());

    // No `x-github-delivery`. GitHub always sends one; a sender that does not
    // still gets a claim key rather than an unclaimed append — which is the
    // invariant this has always held. What the key IS changed: the fleet's id
    // gave every unidentified delivery one shared slot, so the second onward
    // answered `replayed` and never ran. The body's digest keeps the claim and
    // drops the collision, and it is inside what the signature covers.
    let headers = vec![
        (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
    ];
    let response =
        send_with_headers(&router, Method::POST, &path(), None, RUN_FAILURE, &headers).await;

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert_eq!(
        ingress
            .deliveries()
            .first()
            .expect("the delivery was appended")
            .event_id,
        afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
        "an unidentified delivery is claimed under the digest of the body the \
         signature covered, never under the fleet it addressed"
    );
}

/// Two unidentified deliveries with different bodies are two deliveries.
///
/// `x-github-delivery` is NOT covered by the signature — GitHub signs the body
/// alone — so an absent header is a state a sender can produce, and the route
/// has to key the at-most-once claim on something else. Keying it on the fleet
/// would give every unidentified delivery to that fleet ONE shared slot: the
/// first claims it, and every later one answers `replayed` without ever
/// running. A fleet would go quiet and nothing would say why.
///
/// The digest is the answer because it is inside what the signature covers, so
/// it is both attributable and distinct per payload. `app_route` keys on the
/// same digest for the same reason.
#[tokio::test]
async fn unidentified_deliveries_are_told_apart_by_the_body_the_signature_covered() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(
        *field(&json_body(first).await, "replayed"),
        Value::Bool(false)
    );

    let second = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, &other_failure()).await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);
    assert_eq!(
        *field(&json_body(second).await, "replayed"),
        Value::Bool(false),
        "a different payload is a different delivery, however the header reads"
    );

    let appended = ingress.deliveries();
    assert_eq!(appended.len(), 2, "both bodies reached the store");
    let first_id = appended.first().expect("the first append").event_id.clone();
    let second_id = appended.get(1).expect("the second append").event_id.clone();
    assert_ne!(
        first_id, second_id,
        "two bodies must not share one claim key"
    );
    assert!(
        first_id != signed::FLEET && second_id != signed::FLEET,
        "the fleet id must never become a claim key: it is one slot for every \
         unidentified delivery, and the second one onward would be suppressed"
    );
}

/// The same unidentified body twice is still one delivery.
#[tokio::test]
async fn an_unidentified_delivery_repeated_is_still_a_replay() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(
        *field(&json_body(first).await, "replayed"),
        Value::Bool(false)
    );

    let again = deliver_unidentified(&ingress, EVENT_WORKFLOW_RUN, RUN_FAILURE).await;
    assert_eq!(
        *field(&json_body(again).await, "replayed"),
        Value::Bool(true),
        "keying on the digest still suppresses a genuine resend"
    );
}

/// The same failed run, as a different payload.
///
/// A marker key at the top level rather than an edited field: it keeps the
/// document a `workflow_run` failure — so `classify` still accepts it and the
/// two cases differ in exactly one thing, the bytes — while making the digest
/// unmistakably different.
fn other_failure() -> String {
    RUN_FAILURE.replacen('{', r#"{"fixture_marker":"second","#, 1)
}

/// One signed delivery carrying no `x-github-delivery` header at all.
async fn deliver_unidentified(ingress: &Arc<Scripted>, event: &str, body: &str) -> Response {
    let router = Fleet::new().with_ingress(ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, body.as_bytes());
    let headers = vec![
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
        (signed::name(signed::HEADER_EVENT), event),
    ];
    send_with_headers(&router, Method::POST, &path(), None, body, &headers).await
}

/// The delivery header cannot buy a second run of a body already processed.
///
/// GitHub signs the BODY and not the headers, so `x-github-delivery` is
/// unauthenticated: anyone able to resend a captured signed payload can put a
/// fresh value there. Keying the claim on it therefore hands the resender the
/// suppression key itself — a new value per attempt, a new claim per value, and
/// the fleet runs again for each one. That is the whole failure replay
/// suppression exists to prevent, and it needs no forged signature to reach.
///
/// The digest is inside what the signature covers, so it cannot be varied
/// without breaking verification. A genuine GitHub redelivery resends the same
/// body and still lands on the same key, which the redelivery case above
/// asserts; this one asserts the other half.
#[tokio::test]
async fn a_resend_under_a_fresh_delivery_header_is_still_the_same_claim() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());

    let first = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        signed::DELIVERY_ID,
        RUN_FAILURE,
    )
    .await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    // Same signed body, a delivery id the resender picked.
    let second = deliver(
        &ingress,
        EVENT_WORKFLOW_RUN,
        "00000000-0000-4000-8000-000000000000",
        RUN_FAILURE,
    )
    .await;
    assert_eq!(second.status(), StatusCode::ACCEPTED);

    let document = json_body(second).await;
    assert_eq!(
        *field(&document, "replayed"),
        Value::Bool(true),
        "a body already claimed is a replay however the header is spelled — \
         answering false here means the resender chose the suppression key"
    );

    let keys: Vec<_> = ingress
        .deliveries()
        .iter()
        .map(|recorded| recorded.event_id.clone())
        .collect();
    assert_eq!(
        keys,
        [
            afd_ingress::replay_id(RUN_FAILURE.as_bytes()),
            afd_ingress::replay_id(RUN_FAILURE.as_bytes())
        ],
        "both attempts claim under the signed body's digest, so the second \
         finds the first's claim and runs nothing"
    );
}
