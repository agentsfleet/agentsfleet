//! The signed-ingress order, run against the stores it actually resolves through.
//!
//! Every other suite on this surface injects `Scripted`, which answers the three
//! questions from a script the test wrote. That is the right seam for proving
//! what the HANDLER decides, and it is why `afd_http`'s production
//! `impl WebhookIngress for Ingress` and `afd_ingress`'s own resolution read no
//! covered lines: the delegating adapter and the SQL behind it never run.
//!
//! These do run them. The row comes out of `core.fleets`, the secret out of
//! `vault.secrets`, and the claim lands in a live Redis — so a delivery here
//! crosses the same three stores in the same order the daemon crosses them.
//!
//! # What this suite is for, and what `webhook_fleet_route.rs` keeps
//!
//! The route's policy — which events a trigger admits, what a paused fleet is
//! answered, which actor a wake records — is proven with no datastore and stays
//! there. What lives here is only what a stub cannot answer honestly: that the
//! stored document resolves to the binding the reader believes it does, that
//! the sealed credential opens to the secret the wall then verifies against,
//! and that an append reaches the queue.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use crate::harness;

#[path = "ingress_live/fixture.rs"]
mod fixture;

use self::fixture::{CREDENTIAL, Fixture, Runnable, SIGNING_SECRET, SOURCE, Secret, WRONG_SECRET};
use self::harness::webhook as signed;
use self::harness::{json_body, send_with_headers};
use afd_core::error_code;
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A failed run — the delivery that wakes a fleet.
const RUN_FAILURE: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure.json");

/// The event kind the fixture delivery is.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// The reason the route drops a delivery to a fleet nobody is running.
const REASON_FLEET_PAUSED: &str = "fleet_paused";

/// The scheme `github` resolves to — a hex tag over the body alone.
const SCHEME: Scheme = Scheme::BodyHex;

/// Where this fleet's deliveries arrive.
fn path(fleet: &afd_core::id::Uuid7) -> String {
    format!("/v1/webhooks/{fleet}/{SOURCE}")
}

/// One delivery of `RUN_FAILURE`, signed with `secret`.
async fn deliver(
    router: &axum::Router,
    fleet: &afd_core::id::Uuid7,
    secret: &[u8],
) -> axum::response::Response {
    let proof = signed::signature(SCHEME, secret, RUN_FAILURE.as_bytes());
    let headers = vec![
        (signed::name(SCHEME.signature_header()), proof.as_str()),
        (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
        (signed::name(signed::HEADER_DELIVERY), "live-delivery-1"),
    ];
    send_with_headers(
        router,
        Method::POST,
        &path(fleet),
        None,
        RUN_FAILURE,
        &headers,
    )
    .await
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_signed_delivery_resolves_its_fleet_and_secret_out_of_the_live_stores() {
    // The whole order in one pass, and the only test that proves the stores
    // answer at all: the binding is read from `core.fleets`' stored document,
    // the secret is opened out of `vault.secrets`, the tag verifies against it,
    // and the claim reaches Redis. A stub can answer each of those; none of
    // them can be WRONG under a stub, which is what this is here to catch.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();

    let woken = deliver(&router, &fixture.fleet, SIGNING_SECRET).await;
    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(false),
        "the first arrival of a delivery is not a replay: {document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn the_same_delivery_arriving_twice_is_claimed_once() {
    // At-most-once over a REAL Redis, which is the half a stub cannot prove: the
    // claim is a `SET NX` in the queue, so a scripted store asserting "the
    // handler asked for a claim" says nothing about whether the queue would
    // have granted it twice.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();

    let first = deliver(&router, &fixture.fleet, SIGNING_SECRET).await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    assert_eq!(
        json_body(first)
            .await
            .get("replayed")
            .and_then(Value::as_bool),
        Some(false)
    );

    let retried = deliver(&router, &fixture.fleet, SIGNING_SECRET).await;
    let status = retried.status();
    let document = json_body(retried).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("replayed").and_then(Value::as_bool),
        Some(true),
        "the sender's retry of one delivery must not wake the fleet twice: {document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_delivery_signed_with_the_wrong_secret_is_refused_by_the_stored_one() {
    // The refusal has to come from the SEALED bytes rather than from a script,
    // because that is the failure a mis-sealed credential produces in
    // production: the wall verifies against whatever the vault actually opened.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();

    let refused = deliver(&router, &fixture.fleet, WRONG_SECRET).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_SIGNATURE_INVALID.as_str())
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_workspace_holding_no_credential_refuses_before_it_verifies() {
    // `UZ-WH-020`, reached the way a deployment reaches it: the trigger names a
    // credential and the vault holds no row under that name. The reader answers
    // `Ok(None)` rather than raising, because a name that came out of a fleet's
    // own document is a misconfiguration to fail closed on and not an incident.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Absent).await;
    let router = fixture.router();

    let refused = deliver(&router, &fixture.fleet, SIGNING_SECRET).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_CREDENTIAL_NOT_CONFIGURED.as_str()),
        "a fleet whose credential is absent is unconfigured, never a bad \
         signature — the two send an operator to different places: {document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_paused_fleet_is_acknowledged_from_its_stored_status() {
    // The status column is read on the same statement as the document, and this
    // is what proves it is read at all: a paused fleet is answered 200 with a
    // reason rather than refused, because a sender's retry queue adds nothing
    // for a fleet somebody paused on purpose.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Paused, Secret::Stored).await;
    let router = fixture.router();

    let dropped = deliver(&router, &fixture.fleet, SIGNING_SECRET).await;
    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(REASON_FLEET_PAUSED),
        "{document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_fleet_this_deployment_does_not_serve_is_refused_without_naming_it() {
    // No row at all. `Ok(None)` for a fleet with no row AND for one declaring no
    // webhook trigger, answered identically on purpose: telling them apart would
    // confirm a fleet id to whoever guessed it.
    let fixture = Fixture::create().await;
    fixture.seed(Runnable::Active, Secret::Stored).await;
    let router = fixture.router();

    let unknown = afd_core::id::Uuid7::parse("019329c5-0000-7000-8000-0000000000ff")
        .expect("the absent fleet id is canonical");
    let refused = deliver(&router, &unknown, SIGNING_SECRET).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_FLEET_NOT_FOUND.as_str())
    );
    assert!(
        !document.to_string().contains(CREDENTIAL),
        "a refusal must not name what the deployment holds: {document}"
    );

    fixture.cleanup().await;
}
