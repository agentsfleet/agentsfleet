//! `POST /v1/ingress/{provider}` — the deliveries acknowledged and dropped.
//!
//! Split from `app_ingress_route.rs` at the seam the file cap draws and the
//! surface already has: that file proves what wakes a fleet and what is
//! refused. This one proves the 200s that wake nothing — an installation no
//! workspace claims, a kind no fleet subscribes to, a green build, an event
//! this build has no writer for — each read by its REASON, because the status
//! alone is the same for all of them.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the daemon's restriction set is the manifest's"
)]

use crate::app_ingress_route::{
    APP_RUN_FAILURE, APP_SECRET, EVENT_WORKFLOW_RUN, SHIPPED, code, deliver, deployment, subscriber,
};
use crate::harness;

use std::sync::Arc;

use self::harness::webhook as signed;
use self::harness::{Scripted, json_body};
use afd_core::error_code;
use http::StatusCode;
use serde_json::Value;

/// The same run as [`APP_RUN_FAILURE`], green.
const APP_RUN_SUCCESS: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_run_success.json");

/// A terminal production deployment, as an App sends it.
///
/// Minimal on purpose: `octocrab` types `deployment` and `deployment_status` as
/// free-form JSON, so what this fixture has to get right is the ENVELOPE — the
/// installation this deployment's scripted lookup answers for, and a repository
/// a subscriber could match. The payload is here to be classified, and the
/// point of the test is that classification has nowhere to send it.
const APP_DEPLOYMENT_STATUS: &str =
    include_str!("../../../../tests/fixtures/webhooks/github_deployment_status_app.json");

/// The kind a production deployment result is.
///
/// Named here because this build has no writer for what it carries — see the
/// test at the bottom of this file.
const EVENT_DEPLOYMENT_STATUS: &str = "deployment_status";

/// The reason a delivery no rule classifies is answered with.
const REASON_UNSUPPORTED: &str = "unsupported_event";

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
    assert!(
        ingress.deliveries().is_empty(),
        "{:?}",
        ingress.deliveries()
    );
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
    assert!(
        ingress.deliveries().is_empty(),
        "{:?}",
        ingress.deliveries()
    );
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

/// A `deployment_status` delivery is acknowledged and dropped, recording nothing.
///
/// No writer records a deployment, so the delivery falls through classification
/// and is dropped as unsupported. The endpoint's generated description says so,
/// and this is the behaviour behind the sentence —
/// prose and route graded together, because the sentence is the part an
/// integrator acts on.
///
/// Dropped rather than refused, deliberately: a 4xx is what makes a provider
/// retry, and there is nothing here for a retry to achieve.
#[tokio::test]
async fn a_deployment_status_delivery_is_acknowledged_and_records_nothing() {
    let ingress = deployment(vec![subscriber(signed::FLEET)]);
    let answered = deliver(
        &ingress,
        SHIPPED,
        EVENT_DEPLOYMENT_STATUS,
        APP_SECRET,
        APP_DEPLOYMENT_STATUS,
    )
    .await;

    assert_eq!(dropped_for(answered).await, REASON_UNSUPPORTED);
    assert!(
        ingress.deliveries().is_empty(),
        "no run was started for an event this build cannot act on"
    );
}
