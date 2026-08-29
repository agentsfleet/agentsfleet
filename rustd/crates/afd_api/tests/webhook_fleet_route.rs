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
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

mod harness;

use std::sync::Arc;

use afd_core::error_code;
use afd_fleet_lifecycle::FleetStatus;
use afd_ingress::Surface;
use afd_webhook::Scheme;
use harness::webhook as signed;
use harness::{Fleet, Recorded, Scripted, json_body, send_with_headers};
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
const TRIGGER_PUSH_ONLY: &str =
    r#"[{"type":"webhook","source":"github","events":["push"]}]"#;

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

/// The `ignored` reason a 200 carries.
async fn ignored_reason(response: Response) -> String {
    let status = response.status();
    let document = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "a dropped delivery answers 200: {document}");
    document["ignored"]
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
        document["replayed"], Value::Bool(false),
        "the first delivery of an event is not a replay"
    );

    let appended = ingress.deliveries();
    assert_eq!(appended.len(), 1, "one delivery is one append: {appended:?}");
    let Recorded {
        surface,
        fleet,
        event_id,
        actor,
        ..
    } = &appended[0];

    assert_eq!(*surface, Surface::Fleet, "the URL named the fleet, so the claim is the per-fleet one");
    assert_eq!(fleet, signed::FLEET);
    assert_eq!(
        event_id,
        signed::DELIVERY_ID,
        "the claim key is the value GitHub REPEATS on a retry, never one minted here"
    );
    assert_eq!(
        actor, ACTOR_GITHUB,
        "a webhook wake names the provider and no person"
    );
    assert!(
        document["event_id"]
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
        second["event_id"], first["event_id"],
        "a sender comparing two responses must see the same event both times"
    );
    assert_eq!(
        second["replayed"], Value::Bool(true),
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
        [signed::DELIVERY_ID, signed::DELIVERY_ID],
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
        document["error_code"],
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
async fn a_delivery_with_no_identifier_falls_back_to_the_fleet_it_addressed() {
    let ingress = serving(signed::TRIGGER_GITHUB, FleetStatus::Active.as_str());
    let router = Fleet::new().with_ingress(&ingress).router();
    let proof = signed::signature(Scheme::BodyHex, signed::SECRET, RUN_FAILURE.as_bytes());

    // No `x-github-delivery`. GitHub always sends one; a sender that does not
    // still gets a claim key rather than an unclaimed append.
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
        ingress.deliveries()[0].event_id,
        signed::FLEET,
        "with no sender identifier the fleet id is the key, so two such \
         deliveries collapse rather than running the fleet twice"
    );
}
