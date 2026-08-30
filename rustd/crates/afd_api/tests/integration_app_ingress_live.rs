//! Where an App delivery actually goes, resolved over the joins that decide it.
//!
//! The App surface is the one that cannot be addressed. A provider App posts
//! every event for every repository it can see to ONE endpoint, carrying no
//! fleet, no workspace and no principal — so everything about where a delivery
//! lands is looked up: `core.connector_installs` maps the installation to a
//! workspace, and `core.integration_grants` joined against each fleet's stored
//! document decides which fleets in it subscribed.
//!
//! `app_ingress_route.rs` proves the route's policy against a scripted store,
//! which is right for what the HANDLER decides and cannot be wrong about the
//! lookup: a stub told to answer "this workspace" always answers it. A join
//! can. These run the join.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

#[path = "app_ingress_live/fixture.rs"]
mod fixture;

use self::fixture::{APP_SECRET, Fixture, Mapped, OTHER_REPOSITORY, REPOSITORY, WRONG_SECRET};
use self::harness::webhook as signed;
use self::harness::{json_body, send_with_headers};
use afd_core::error_code;
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A failed run delivered through an App installation.
const RUN_FAILURE_APP: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_failure_app.json");

/// Where one App's deliveries arrive.
const PATH: &str = "/v1/ingress/github";

/// The event kind the fixture delivery is.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// One App delivery of `body`, signed with `secret`.
async fn deliver(router: &axum::Router, body: &str, secret: &[u8]) -> axum::response::Response {
    let proof = signed::signature(Scheme::BodyHex, secret, body.as_bytes());
    let headers = vec![
        (
            signed::name(Scheme::BodyHex.signature_header()),
            proof.as_str(),
        ),
        (signed::name(signed::HEADER_EVENT), EVENT_WORKFLOW_RUN),
        (signed::name(signed::HEADER_DELIVERY), signed::DELIVERY_ID),
    ];
    send_with_headers(router, Method::POST, PATH, None, body, &headers).await
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_app_delivery_wakes_the_fleet_the_joins_route_it_to() {
    // The whole lookup in one pass. The App secret is opened out of the PLATFORM
    // ADMIN workspace, the installation is resolved to a tenant workspace
    // through `core.connector_installs`, and the fleets are narrowed by an
    // approved grant and then by each stored document's repository list.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::ToWorkspace).await;
    let router = fixture.router();

    let woken = deliver(&router, &fixture.delivery(RUN_FAILURE_APP), APP_SECRET).await;
    let status = woken.status();
    let document = json_body(woken).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(
        document.get("matched").and_then(Value::as_u64),
        Some(1),
        "exactly the one subscribed fleet matched, which is the evidence the \
         join narrowed rather than defaulted: {document}"
    );
    assert_eq!(
        document.get("enqueued").and_then(Value::as_u64),
        Some(1),
        "a matched fleet that was not enqueued would be a delivery silently \
         dropped after the routing said it belonged somewhere: {document}"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_installation_this_deployment_never_mapped_is_dropped_not_refused() {
    // An App installed on an organisation that never finished connecting. The
    // sender is a correctly configured provider with nothing to fix, so this is
    // a drop rather than a refusal — a 4xx would put a delivery it cannot stop
    // sending into a retry loop that can never succeed.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::Nowhere).await;
    let router = fixture.router();

    let before = fixture.fleet_events().await;
    let dropped = deliver(&router, &fixture.delivery(RUN_FAILURE_APP), APP_SECRET).await;
    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(error_code::WEBHOOK_INSTALL_NOT_MAPPED.as_str()),
        "{document}"
    );
    assert_eq!(
        fixture.fleet_events().await,
        before,
        "an unmapped installation must wake nothing"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_repository_no_fleet_subscribed_to_wakes_nobody() {
    // The document half of the narrowing, over a real row. The workspace maps,
    // the grant is approved and the event kind is admitted — only the
    // repository differs — so a fleet woken here would be one woken by another
    // team's repository inside its own workspace.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::ToWorkspace).await;
    let router = fixture.router();

    let elsewhere = fixture
        .delivery(RUN_FAILURE_APP)
        .replace(REPOSITORY, OTHER_REPOSITORY);
    assert!(
        elsewhere.contains(OTHER_REPOSITORY) && !elsewhere.contains(REPOSITORY),
        "the fixture substitution must actually change the repository"
    );

    let before = fixture.fleet_events().await;
    let dropped = deliver(&router, &elsewhere, APP_SECRET).await;
    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND.as_str()),
        "{document}"
    );
    assert_eq!(
        fixture.fleet_events().await,
        before,
        "a repository nobody subscribed to must wake nothing"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_delivery_signed_with_the_wrong_secret_never_reaches_the_joins() {
    // The wall runs before any routing, and this is what proves the ORDER over
    // real stores: an installation that maps and a fleet that subscribes are
    // both present, and neither is consulted.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::ToWorkspace).await;
    let router = fixture.router();

    let before = fixture.fleet_events().await;
    let refused = deliver(&router, &fixture.delivery(RUN_FAILURE_APP), WRONG_SECRET).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::WEBHOOK_SIGNATURE_INVALID.as_str())
    );
    assert!(
        !document.to_string().contains(fixture.installation()),
        "a refusal must not confirm which installations this deployment knows: \
         {document}"
    );
    assert_eq!(
        fixture.fleet_events().await,
        before,
        "an unproven delivery must wake nothing"
    );

    fixture.cleanup().await;
}
