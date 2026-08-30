//! What an approval callback WRITES, once it is past the wall.
//!
//! `webhook_approval_wall.rs` proves the refusals, and it proves them with no
//! datastore precisely because none of them may reach one. This file is the
//! other half: every case here gets past the signature and is judged on the
//! rows it left behind.
//!
//! # The continuation is the assertion, not the status code
//!
//! A resolved gate that lands no continuation is a run a person unblocked and
//! nothing restarted — the fleet sits parked forever while the dashboard shows
//! an answered gate. That failure is invisible to the response body, which says
//! `resolved` either way, so every test here reads `core.fleet_events` rather
//! than trusting the 200.
//!
//! # Why the callback is compared against the bearer surface
//!
//! Two doors resolve one gate: a person clicking in the dashboard and a person
//! clicking in Slack. They must leave the same row, or a gate's history depends
//! on which button was nearer. `both_doors_leave_the_same_row` is that
//! comparison, and it is the reason this file seeds two identical gates.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "integration preconditions should fail the test loudly"
)]

use crate::harness;

use afd_core::error_code;
use afd_webhook::Scheme;
use http::{Method, StatusCode};
use serde_json::Value;

use self::harness::webhook as signed;
use self::harness::{json_body, send, send_with_headers};
#[path = "approval_callback_live/fixture.rs"]
mod fixture;

use self::fixture::Fixture;

/// The secret this deployment signs approval callbacks with.
///
/// Sealed into the admin workspace's vault by [`Fixture::seed`] under
/// [`APPROVAL_KEY`], which is the only reason the router can open it.
const PLATFORM_SECRET: &[u8] = b"fixture-approval-callback-secret";

/// What a gate's `resolved_by` says when Slack answered it.
const BY_SLACK_WEBHOOK: &str = "slack:webhook";

/// The instant the router calls now, frozen so a signature is not racing it.
fn now() -> i64 {
    harness::frozen_unix_seconds()
}

/// An approver's answer, as Slack posts it.
fn payload(action: &str, decision: &str) -> String {
    format!(r#"{{"action_id":"{action}","decision":"{decision}"}}"#)
}

/// One signed callback at `fleet`'s path, carrying `body`.
async fn callback(router: &axum::Router, fleet: &str, body: &str) -> axum::response::Response {
    let at = now().to_string();
    let proof = signed::signature_at(Scheme::SlackV0, PLATFORM_SECRET, Some(&at), body.as_bytes());
    let path = format!("/v1/webhooks/{fleet}/approval");
    let headers = signed::approval_headers(&proof, &at);
    send_with_headers(router, Method::POST, &path, None, body, &headers).await
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn an_approved_callback_resolves_the_gate_and_continues_the_run() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router().await;

    let answered = callback(&router, &fixture.fleet, &payload(&fixture.action, "approve")).await;
    let status = answered.status();
    let document = json_body(answered).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some("resolved")
    );
    assert_eq!(
        document.get("action_id").and_then(Value::as_str),
        Some(fixture.action.as_str())
    );

    let (state, by) = fixture.gate_state(&fixture.gate).await;
    assert_eq!(state, "approved");
    assert_eq!(
        by, BY_SLACK_WEBHOOK,
        "the audit column names the door, not the person Slack showed"
    );
    assert_eq!(
        fixture.continuations(&fixture.event).await,
        1,
        "an approved gate that lands no continuation is a run nothing restarted"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_redelivered_callback_continues_the_run_exactly_once() {
    // Slack retries any non-2xx and some 2xx besides, so this is the ordinary
    // case rather than an adversarial one. The gate is already resolved on the
    // second pass, which takes `AlreadyResolved` — the arm that answers 200
    // and must not write a second continuation.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router().await;
    let body = payload(&fixture.action, "approve");

    let first = callback(&router, &fixture.fleet, &body).await;
    assert_eq!(first.status(), StatusCode::OK);
    let again = callback(&router, &fixture.fleet, &body).await;
    let status = again.status();
    let document = json_body(again).await;
    assert_eq!(status, StatusCode::OK, "{document}");
    assert_eq!(
        document.get("status").and_then(Value::as_str),
        Some("resolved"),
        "a retry is told the standing decision rather than refused"
    );

    assert_eq!(
        fixture.continuations(&fixture.event).await,
        1,
        "a retry resumes the run exactly once: twice runs the approved action \
         twice, and never leaves the fleet parked on an answered gate"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_denial_resolves_the_gate_and_starts_nothing() {
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router().await;

    let answered = callback(&router, &fixture.fleet, &payload(&fixture.action, "deny")).await;
    assert_eq!(answered.status(), StatusCode::OK);

    let (state, _by) = fixture.gate_state(&fixture.gate).await;
    assert_eq!(state, "denied");
    assert_eq!(
        fixture.continuations(&fixture.event).await,
        0,
        "a denial is the end of the run, so resuming it would run the very \
         thing a person refused"
    );

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn a_gate_of_another_fleet_is_not_resolvable_through_this_fleets_path() {
    // The fleet in the URL is a FILTER bound into the resolving statement's
    // WHERE, not a lookup. Without it, anyone holding the deployment's signing
    // secret could resolve any gate in the deployment by naming its action —
    // and the secret is shared by every fleet, so "holding it" is the normal
    // state rather than a compromise.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router().await;

    let body = payload(&fixture.other_action, "approve");
    let refused = callback(&router, &fixture.fleet, &body).await;
    let status = refused.status();
    let document = json_body(refused).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{document}");
    assert_eq!(
        document.get("error_code").and_then(Value::as_str),
        Some(error_code::APPROVAL_NOT_FOUND.as_str())
    );

    let (state, _by) = fixture.gate_state(&fixture.other_gate).await;
    assert_eq!(state, "pending", "the other fleet's gate is untouched");
    assert_eq!(fixture.continuations(&fixture.other_event).await, 0);

    fixture.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn both_doors_leave_the_same_row() {
    // Slack's callback and the dashboard's bearer route resolve two gates that
    // differ in nothing but their identifiers. Everything the two rows disagree
    // on afterwards is something that depends on which button was nearer, which
    // is what this asserts there is none of — beyond the audit columns, whose
    // whole job is to differ.
    let fixture = Fixture::create().await;
    fixture.seed().await;
    let router = fixture.router().await;

    let body = payload(&fixture.action, "approve");
    let through_slack = callback(&router, &fixture.fleet, &body).await;
    assert_eq!(through_slack.status(), StatusCode::OK);

    let item = format!(
        "/v1/workspaces/{}/approvals/{}/approve",
        fixture.other_workspace.as_str(),
        fixture.other_gate
    );
    let clicked = send(&router, Method::POST, &item, Some(&fixture.token), "").await;
    let status = clicked.status();
    let document = json_body(clicked).await;
    assert_eq!(status, StatusCode::OK, "{document}");

    let (slack_state, slack_by) = fixture.gate_state(&fixture.gate).await;
    let (dashboard_state, dashboard_by) = fixture.gate_state(&fixture.other_gate).await;
    assert_eq!(
        slack_state, dashboard_state,
        "one gate's state cannot depend on which door answered it"
    );
    assert_eq!(slack_by, BY_SLACK_WEBHOOK);
    assert_eq!(
        dashboard_by, fixture.subject,
        "the dashboard records the person, where Slack records the door"
    );
    assert_eq!(fixture.continuations(&fixture.event).await, 1);
    assert_eq!(
        fixture.continuations(&fixture.other_event).await,
        1,
        "both doors resume the run they unblocked"
    );

    fixture.cleanup().await;
}
