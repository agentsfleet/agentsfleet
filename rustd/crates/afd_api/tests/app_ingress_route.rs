//! `POST /v1/ingress/{provider}` — one App's deliveries, fanned out.
//!
//! Where the per-fleet routes serve a fleet whose id the URL carries, this
//! serves an INSTALLATION whose fleets have to be looked up: a provider App
//! posts every event for every repository in an organisation to one URL, signed
//! with one secret belonging to this deployment rather than to any workspace.
//!
//! # Almost every answer is a 200, and that is the thing worth testing
//!
//! An App receives far more than it is asked to act on — events for
//! repositories nobody subscribed, kinds no fleet declared, installations
//! connected to no workspace, green builds. All correctly signed, all real,
//! none of them waking anything. Each is acknowledged with its reason, because
//! a 4xx would put a delivery nobody can act on into a three-day retry loop and
//! change none of them.
//!
//! So a status code alone proves almost nothing here: 200 is the answer for
//! "woken" and for six different flavours of "dropped". Every case below reads
//! the REASON, and the ones that should wake a fleet read the append log.
//!
//! # No datastore, and that is a property of the seam
//!
//! The route's every decision past the signature is a function of what the
//! ingress seam answered — which installation maps to which workspace, which
//! fleets subscribe — so the whole matrix is reachable with the store scripted
//! and no Postgres or Redis anywhere.

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
use afd_ingress::{MAX_FANOUT, Surface};
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A delivery an App sends for an installation, carrying its own id.
///
/// The one fixture in the corpus written for this surface and, until now,
/// referenced by nothing. Its `installation.id` is [`signed::INSTALLATION`],
/// which is what lets the scripted lookup answer for it.
const APP_RUN_FAILURE: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure_app.json");

/// The same run, green.
const APP_RUN_SUCCESS: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_success.json");

/// What an App sends to prove the endpoint answers.
const PING: &str = include_str!("../../../../tests/fixtures/webhooks/github_ping.json");

/// The delivery kind both run fixtures are.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// The kind a ping is.
const EVENT_PING: &str = "ping";

/// The secret this DEPLOYMENT's App signs every installation's deliveries with.
const APP_SECRET: &[u8] = b"fixture-github-app-secret";

/// The provider this daemon serves an App ingress for.
const SHIPPED: &str = "github";

/// One it does not, spelled as a person might guess it.
const UNSHIPPED: &str = "dropbox";

/// What an App-driven wake records as the actor.
///
/// The App, never the person whose push produced the event: recording the
/// person would let an actor-shaped assertion certify that a human woke this
/// fleet when an installation did.
const ACTOR_APP: &str = "github-app";

/// What a ping is answered with.
const STATUS_PONG: &str = "pong";

/// Where an App's deliveries arrive.
fn path(provider: &str) -> String {
    format!("/v1/ingress/{provider}")
}

/// The registry code, as it is spelled on the wire.
fn code(code: ErrorCode) -> String {
    code.as_str().to_owned()
}

/// A deployment holding the App secret, its installation, and its subscribers.
fn deployment(subscribers: Vec<afd_ingress::Binding>) -> Arc<Scripted> {
    Arc::new(
        Scripted::new()
            .app_signing(APP_SECRET)
            .installed_in(signed::id(signed::WORKSPACE))
            .subscribed(subscribers),
    )
}

/// The fleets a delivery fans out to, each declaring GitHub with no allow-list.
fn subscriber(fleet: &str) -> afd_ingress::Binding {
    signed::binding_of(fleet, signed::TRIGGER_GITHUB, "active")
}

/// One delivery of `body` as `event`, signed with `secret`.
async fn deliver(
    ingress: &Arc<Scripted>,
    provider: &str,
    event: &str,
    secret: &[u8],
    body: &str,
) -> axum::response::Response {
    // The App secret belongs to the DEPLOYMENT, so the wall reads it through
    // the platform admin workspace and refuses `UZ-WH-020` without one.
    let router = Fleet::new()
        .with_ingress(ingress)
        .with_platform_admin(signed::id(signed::WORKSPACE))
        .router();
    let proof = signed::signature(Scheme::BodyHex, secret, body.as_bytes());
    let headers = signed::github_headers(event, signed::DELIVERY_ID, &proof);
    send_with_headers(&router, Method::POST, &path(provider), None, body, &headers).await
}

/// The reason a 200 carries, which is the whole answer on this surface.
async fn dropped_for(response: axum::response::Response) -> String {
    let status = response.status();
    let document = json_body(response).await;
    assert_eq!(status, StatusCode::OK, "a drop is acknowledged: {document}");
    document
        .get("ignored")
        .and_then(Value::as_str)
        .expect("a dropped delivery names why")
        .to_owned()
}

#[tokio::test]
async fn a_signed_delivery_wakes_every_subscribed_fleet_exactly_once() {
    let ingress = deployment(vec![
        subscriber(signed::FLEET),
        subscriber(signed::OTHER_FLEET),
    ]);
    let woken = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;

    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(document.get("matched").and_then(Value::as_u64), Some(2));
    assert_eq!(document.get("enqueued").and_then(Value::as_u64), Some(2));

    let appends = ingress.deliveries();
    assert_eq!(appends.len(), 2, "one append per subscribed fleet");
    for append in &appends {
        assert_eq!(append.surface, Surface::App, "the App's own claim window");
        assert_eq!(
            append.actor, ACTOR_APP,
            "the installation woke this fleet, not the person who pushed"
        );
    }
    let ids: std::collections::BTreeSet<&str> = appends
        .iter()
        .map(|append| append.event_id.as_str())
        .collect();
    assert_eq!(
        ids.len(),
        1,
        "one delivery is one event however many fleets it reaches, so a \
         redelivery suppresses all of them or none"
    );
}

#[tokio::test]
async fn a_redelivery_wakes_nobody_a_second_time() {
    // GitHub retries for three days and an operator can press Redeliver by
    // hand inside that window. Each fleet's claim is its own, so the answer
    // still reports the full matched set — what changes is that none of them
    // were enqueued again.
    let ingress = deployment(vec![
        subscriber(signed::FLEET),
        subscriber(signed::OTHER_FLEET),
    ]);
    let first = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let again = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;
    let status = again.status();
    let document = json_body(again).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("matched").and_then(Value::as_u64),
        Some(2),
        "the delivery still matched both fleets"
    );
    assert_eq!(
        document.get("enqueued").and_then(Value::as_u64),
        Some(0),
        "and woke neither, because both claims were already held"
    );
}

#[tokio::test]
async fn a_ping_is_answered_only_on_the_far_side_of_the_signature() {
    // A ping proves the endpoint to whoever configured it. Answering one
    // before the signature would tell a prober the path exists.
    let ingress = deployment(vec![subscriber(signed::FLEET)]);

    let proven = deliver(&ingress, SHIPPED, EVENT_PING, APP_SECRET, PING).await;
    let status = proven.status();
    let document = json_body(proven).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some(STATUS_PONG)
    );

    let forged = deliver(&ingress, SHIPPED, EVENT_PING, signed::WRONG_SECRET, PING).await;
    let status = forged.status();
    let document = json_body(forged).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_SIGNATURE_INVALID))
    );
    assert!(
        !document.to_string().contains(STATUS_PONG),
        "an unverified ping must not be told the path answers: {document}"
    );
    assert!(ingress.deliveries().is_empty(), "and must wake nothing");
}

#[tokio::test]
async fn a_delivery_for_an_installation_no_workspace_claims_is_dropped_not_refused() {
    // An App stays installed after a workspace disconnects it, and keeps
    // posting. Refusing would retry-loop a delivery the sender cannot fix.
    let ingress = Arc::new(Scripted::new().app_signing(APP_SECRET));
    let answered = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;

    assert_eq!(
        dropped_for(answered).await,
        code(error_code::WEBHOOK_INSTALL_NOT_MAPPED)
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_delivery_no_fleet_subscribes_to_is_dropped() {
    let ingress = deployment(Vec::new());
    let answered = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;

    assert_eq!(
        dropped_for(answered).await,
        code(error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND)
    );
    assert!(ingress.deliveries().is_empty());
}

#[tokio::test]
async fn a_green_run_is_dropped_rather_than_woken_on() {
    // The classification runs on the App surface too, and it is the reason
    // most App traffic costs nothing: a successful build is the common case.
    let ingress = deployment(vec![subscriber(signed::FLEET)]);
    let answered = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_SUCCESS,
    )
    .await;

    let reason = dropped_for(answered).await;
    assert!(!reason.is_empty(), "a drop always names why");
    assert!(
        ingress.deliveries().is_empty(),
        "a fleet woken by a green build burns a run on nothing to repair"
    );
}

#[tokio::test]
async fn a_matched_set_past_the_ceiling_is_refused_rather_than_truncated() {
    // Waking the first hundred of a hundred and one is a silent,
    // order-dependent choice about whose fleet runs. The operator who wired it
    // that way is the one who has to know.
    let ingress = Arc::new(
        Scripted::new()
            .app_signing(APP_SECRET)
            .installed_in(signed::id(signed::WORKSPACE))
            .matching(MAX_FANOUT + 1),
    );
    let refused = deliver(
        &ingress,
        SHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert!(status.is_client_error(), "{status} {document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND))
    );
    assert!(
        ingress.deliveries().is_empty(),
        "none of them, not the first hundred"
    );
}

#[tokio::test]
async fn a_path_naming_no_served_app_is_refused_before_anything_is_read() {
    let ingress = deployment(vec![subscriber(signed::FLEET)]);
    let refused = deliver(
        &ingress,
        UNSHIPPED,
        EVENT_WORKFLOW_RUN,
        APP_SECRET,
        APP_RUN_FAILURE,
    )
    .await;

    let status = refused.status();
    let document = json_body(refused).await;
    assert!(status.is_client_error(), "{status} {document}");
    assert_eq!(
        document
            .get("error_code")
            .and_then(Value::as_str)
            .map(str::to_owned),
        Some(code(error_code::CONNECTOR_UNKNOWN))
    );
}

#[tokio::test]
async fn a_delivery_that_is_not_the_event_its_header_claims_is_malformed() {
    // Verified, and still unreadable: the signature proves who sent it, never
    // that the body is the kind the header says. This is the one refusal past
    // the wall, and it is a 4xx because a sender genuinely can fix it.
    let ingress = deployment(vec![subscriber(signed::FLEET)]);
    let refused = deliver(&ingress, SHIPPED, EVENT_WORKFLOW_RUN, APP_SECRET, PING).await;

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
