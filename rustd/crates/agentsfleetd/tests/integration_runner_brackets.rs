//! The bracket frames — the daemon opens and closes a run on the tail itself.
//!
//! `event_received` when the lease verb writes the row, `event_complete` when
//! a report or a refusal closes it. Proven end to end because the property is
//! the ORDER: the row is written before the frame that names it, so a watcher
//! reacting to the frame can never find the row missing. Beside the runner's
//! forwarded frames (`integration_runner_activity.rs`) rather than inside that
//! suite, so each file stays about one publisher.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_redis::SubscriptionHub;
use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::{redis_config, scenario};
use crate::tail::{lease, next_frame, settle};
use crate::wire::{capable_beat, field, json, post, report_body};

/// The bracket frames — the daemon opens and closes a run on the tail itself.
///
/// `event_received` when the lease verb writes the row, `event_complete` when
/// the report closes it, and the second carries the row as the events list
/// would serve it plus the fleet's status and pending gate count. Proven end
/// to end because the property is the ORDER: the row is written before the
/// frame that names it, so a watcher reacting to the frame can never find
/// the row missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_bracket_frames_open_and_close_a_run() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    // Subscribed BEFORE the lease: the opening bracket is published by the
    // poll itself, and pub/sub keeps nothing for a reader that arrives late.
    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    settle().await;

    let (lease_id, fence) = lease(&http, &run).await;
    let opened = next_frame(&mut tail)
        .await
        .expect("the lease announces the row it opened");
    assert_eq!(field(&opened, "kind"), &json!("event_received"));
    assert_eq!(
        field(&opened, "event_id"),
        &json!(run.event_id),
        "the frame names the event the lease was issued for"
    );
    assert!(field(&opened, "actor").is_string());
    assert!(field(&opened, "event_type").is_string());
    assert!(
        field(&opened, "created_at").is_i64(),
        "the row's own instant, so a client never stamps it with its clock"
    );

    let reported = post(
        &http,
        &run,
        "/v1/runners/me/reports",
        &report_body(&lease_id, &run.event_id, fence),
    )
    .await;
    assert_eq!(reported.status().as_u16(), 200, "the report settles");

    let closed = next_frame(&mut tail)
        .await
        .expect("the report announces the row it closed");
    assert_eq!(field(&closed, "kind"), &json!("event_complete"));
    assert_eq!(field(&closed, "event_id"), &json!(run.event_id));
    assert_eq!(field(&closed, "status"), &json!("processed"));
    assert_eq!(
        field(&closed, "wall_ms"),
        &json!(1_500),
        "the runner's telemetry rides the row"
    );
    assert!(
        field(&closed, "tokens")
            .as_i64()
            .is_some_and(|tokens| tokens > 0),
        "so does its token count"
    );
    assert!(
        field(&closed, "cost_nanos").is_i64(),
        "the settle's ledger rows are summed into the frame, as the events list sums them"
    );
    assert_eq!(
        field(&closed, "fleet_status"),
        &json!("active"),
        "the fleet's status after the run, for the console's lifecycle controls"
    );
    assert_eq!(
        field(&closed, "pending_approvals"),
        &json!(0),
        "and how many approvals wait on it, for the console's strip"
    );

    drop(tail);
    supervisor.shutdown().await;
    run.cleanup().await;
}

/// A lease refused at a gate closes the run on the tail too.
///
/// The money gate is the refusal reachable over HTTP: an exhausted tenant's
/// poll ends the event `gate_blocked`, and the watcher must learn that from
/// the tail rather than from a reload — the row was opened by the same poll,
/// so both brackets arrive from one request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_a_refused_lease_closes_the_run_on_the_tail() {
    let mut supervisor = Supervisor::new();
    let run = scenario(&mut supervisor).await;
    let http = reqwest::Client::new();

    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", run.fleet));
    settle().await;

    run.drain_wallet().await;
    let beat = post(&http, &run, "/v1/runners/me/heartbeats", &capable_beat()).await;
    assert_eq!(beat.status().as_u16(), 200);
    let polled = post(&http, &run, "/v1/runners/me/leases", &json!({})).await;
    assert_eq!(polled.status().as_u16(), 200);
    assert_eq!(
        field(&json(polled).await, "lease"),
        &json!(null),
        "an exhausted tenant receives no lease"
    );

    let opened = next_frame(&mut tail)
        .await
        .expect("the row is opened before the money is checked");
    assert_eq!(field(&opened, "kind"), &json!("event_received"));
    let closed = next_frame(&mut tail)
        .await
        .expect("and the refusal closes it on the same tail");
    assert_eq!(field(&closed, "kind"), &json!("event_complete"));
    assert_eq!(field(&closed, "event_id"), &json!(run.event_id));
    assert_eq!(field(&closed, "status"), &json!("gate_blocked"));
    assert!(
        field(&closed, "failure_label")
            .as_str()
            .is_some_and(|label| !label.is_empty()),
        "the refusal names what refused it, so the chat can say so live"
    );
    assert_eq!(
        field(&closed, "tokens"),
        &json!(null),
        "a run that never started spent nothing"
    );

    drop(tail);
    supervisor.shutdown().await;
    run.cleanup().await;
}
