//! Where an App delivery actually goes, resolved over the joins that decide it.
//!
//! The App surface is the one that cannot be addressed. A provider App posts
//! every event for every repository it can see to ONE endpoint, carrying no
//! fleet, no workspace and no principal — so everything about where a delivery
//! lands is looked up: `connector.installs` maps the installation to a
//! workspace, and each fleet's `app_event` row in `fleet.triggers` decides which
//! fleets in it subscribed. No grant is read on the way.
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
use af_core::error_code;
use af_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

/// A failed run delivered through an App installation.
const RUN_FAILURE_APP: &str = afd_api_ingress::test_util::GITHUB_RUN_FAILURE_APP;

/// Where one App's deliveries arrive.
const PATH: &str = "/v1/ingress/github";

/// The event kind the fixture delivery is.
const EVENT_WORKFLOW_RUN: &str = "workflow_run";

/// The actor an App delivery's turn is admitted from.
const APP_ACTOR: &str = "github-app";

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
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_github_trigger_routes_an_app_delivery() {
    // The whole lookup in one pass. The App secret is opened out of the PLATFORM
    // ADMIN workspace, the installation is resolved to a tenant workspace
    // through `connector.installs`, and the fleets are narrowed by their
    // `app_event` row's repository list. The fleet holds no grant.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::ToWorkspace).await;
    let router = fixture.router();

    let body = fixture.delivery(RUN_FAILURE_APP);
    let woken = deliver(&router, &body, APP_SECRET).await;
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

    assert_eq!(
        fixture.turn_actors().await,
        vec![APP_ACTOR.to_owned()],
        "one turn, admitted from the App"
    );

    let replay = deliver(&router, &body, APP_SECRET).await;
    let status = replay.status();
    let document = json_body(replay).await;
    assert_eq!(status, StatusCode::ACCEPTED, "{document}");
    assert_eq!(document.get("enqueued").and_then(Value::as_u64), Some(0));
    assert_eq!(fixture.turn_actors().await.len(), 1, "a replay is no turn");

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_installation_this_deployment_never_mapped_is_dropped_not_refused() {
    // An App installed on an organisation that never finished connecting. The
    // sender is a correctly configured provider with nothing to fix, so this is
    // a drop rather than a refusal — a 4xx would put a delivery it cannot stop
    // sending into a retry loop that can never succeed.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::Nowhere).await;
    let router = fixture.router();

    let before = fixture.fleet_turns().await;
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
        fixture.fleet_turns().await,
        before,
        "an unmapped installation must wake nothing"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_app_delivery_outside_a_trigger_reaches_no_fleet() {
    // The row half of the narrowing, over a real row. The workspace maps and
    // the event kind is admitted — only the repository differs — so a fleet
    // woken here would be one woken by another team's repository inside its
    // own workspace.
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

    let before = fixture.fleet_turns().await;
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
        fixture.fleet_turns().await,
        before,
        "a repository nobody subscribed to must wake nothing"
    );
    fixture.cleanup().await;

    // The subscribed repository, from an install another workspace connected:
    // that workspace holds no row, and this one's row is not read.
    let crossed = Fixture::create().await;
    crossed.seed(Mapped::ToOtherWorkspace).await;
    let router = crossed.router();
    let dropped = deliver(&router, &crossed.delivery(RUN_FAILURE_APP), APP_SECRET).await;
    let status = dropped.status();
    let document = json_body(dropped).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("ignored").and_then(Value::as_str),
        Some(error_code::WEBHOOK_SUBSCRIPTION_NOT_FOUND.as_str()),
        "{document}"
    );
    assert!(
        crossed.turn_actors().await.is_empty(),
        "another workspace's install reaches no fleet of this one"
    );
    crossed.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_delivery_signed_with_the_wrong_secret_never_reaches_the_joins() {
    // The wall runs before any routing, and this is what proves the ORDER over
    // real stores: an installation that maps and a fleet that subscribes are
    // both present, and neither is consulted.
    let fixture = Fixture::create().await;
    fixture.seed(Mapped::ToWorkspace).await;
    let router = fixture.router();

    let before = fixture.fleet_turns().await;
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
        fixture.fleet_turns().await,
        before,
        "an unproven delivery must wake nothing"
    );

    fixture.cleanup().await;
}
