//! The bracket frames — the daemon opens and closes a run on the tail itself.
//!
//! `turn_received` when run issuance writes the row, `turn_complete` when
//! a report or a refusal closes it. Proven end to end because the property is
//! the ORDER: the row is written before the frame that names it, so a watcher
//! reacting to the frame can never find the row missing. Beside the runner's
//! forwarded frames (`integration_runner_activity/mod.rs`) rather than inside that
//! suite, so each file stays about one publisher.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use agentsfleetd::supervisor::Supervisor;
use serde_json::json;

use crate::e2e::scenario;
use crate::reads::event_column;
use crate::tail::{next_frame, run, settle};
use crate::wire::{capable_beat, field, report_body, wait_without_assignment};

/// The bracket frames — the daemon opens and closes a run on the tail itself.
///
/// `turn_received` when run issuance writes the row, `turn_complete` when
/// the report closes it, and the second carries the row as the events list
/// would serve it plus the fleet's status and pending gate count. Proven end
/// to end because the property is the ORDER: the row is written before the
/// frame that names it, so a watcher reacting to the frame can never find
/// the row missing.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_bracket_frames_open_and_close_a_run() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;

    // Subscribed BEFORE the run: the opening bracket is published by the
    // claim itself, and pub/sub keeps nothing for a reader that arrives late.
    let mut tail = crate::tail::tail_of(&scene.fleet).await;
    settle().await;

    let (run_id, fence) = run(&scene).await;
    let opened = next_frame(&mut tail)
        .await
        .expect("the run announces the row it opened");
    assert_opened(&opened, &scene.turn_id);

    let reported = crate::wire::report(&scene, &report_body(&run_id, &scene.turn_id, fence)).await;
    assert!(reported.is_ok(), "native report refused: {reported:?}");

    let closed = next_frame(&mut tail)
        .await
        .expect("the report announces the row it closed");
    assert_completed(&closed, &scene.turn_id);

    drop(tail);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

/// A run refused at a gate closes the run on the tail too.
///
/// An exhausted tenant's admission ends the event `gate_blocked`. The same
/// claim opens and closes its row, so a tail watcher receives both brackets.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_a_refused_run_closes_the_run_on_the_tail() {
    let mut supervisor = Supervisor::new();
    let scene = scenario(&mut supervisor).await;

    let mut tail = crate::tail::tail_of(&scene.fleet).await;
    settle().await;

    scene.drain_wallet().await;
    let beat = crate::wire::heartbeat(&scene, &capable_beat()).await;
    assert!(
        beat.get("assigned_policy").is_some(),
        "the native heartbeat carries its assignment"
    );
    // No work may be assigned before the durable money-gate refusal is visible.
    let refused = wait_without_assignment(&scene, async || {
        event_column(&scene, &scene.turn_id, "status")
            .await
            .as_deref()
            == Some("gate_blocked")
    })
    .await;
    assert!(
        refused,
        "the exhausted tenant's claim ends the event at the money gate"
    );

    let opened = next_frame(&mut tail)
        .await
        .expect("the row is opened before the money is checked");
    assert_eq!(field(&opened, "kind"), &json!("turn_received"));
    let closed = next_frame(&mut tail)
        .await
        .expect("and the refusal closes it on the same tail");
    assert_refused(&closed, &scene.turn_id);

    drop(tail);
    supervisor.shutdown().await;
    scene.cleanup().await;
}

fn assert_opened(opened: &serde_json::Value, event: &af_core::id::Uuid7) {
    assert_eq!(field(opened, "kind"), &json!("turn_received"));
    assert_eq!(
        field(opened, "turn_id"),
        &json!(event),
        "the frame names the event the run was issued for"
    );
    assert!(field(opened, "actor").is_string());
    assert!(field(opened, "event_type").is_string());
    assert!(
        field(opened, "created_at")
            .as_str()
            .is_some_and(|at| at.parse::<jiff::Timestamp>().is_ok()),
        "the row's own instant, so a client never stamps it with its clock"
    );
}

fn assert_completed(closed: &serde_json::Value, event: &af_core::id::Uuid7) {
    assert_eq!(field(closed, "kind"), &json!("turn_complete"));
    assert_eq!(field(closed, "turn_id"), &json!(event));
    assert_eq!(field(closed, "status"), &json!("processed"));
    assert_eq!(
        field(closed, "wall_ms"),
        &json!(1_500),
        "the runner's telemetry rides the row"
    );
    assert!(
        field(closed, "tokens")
            .as_i64()
            .is_some_and(|tokens| tokens > 0),
        "so does its token count"
    );
    assert!(
        field(closed, "cost_nanos").is_string(),
        "the settle's ledger rows are summed into the frame, as the events list sums them"
    );
    assert_eq!(
        field(closed, "fleet_status"),
        &json!("active"),
        "the fleet's status after the scene, for the console's lifecycle controls"
    );
    assert_eq!(
        field(closed, "pending_approvals"),
        &json!(0),
        "and how many approvals wait on it, for the console's strip"
    );
}

fn assert_refused(closed: &serde_json::Value, event: &af_core::id::Uuid7) {
    assert_eq!(field(closed, "kind"), &json!("turn_complete"));
    assert_eq!(field(closed, "turn_id"), &json!(event));
    assert_eq!(field(closed, "status"), &json!("gate_blocked"));
    assert!(
        field(closed, "failure_label")
            .as_str()
            .is_some_and(|label| !label.is_empty()),
        "the refusal names what refused it, so the chat can say so live"
    );
    assert_eq!(
        field(closed, "tokens"),
        &json!(null),
        "a scene that never started spent nothing"
    );
}
