//! What the tail says for a gate that held no run.
//!
//! Split from `integration_inbox_tail.rs` at the 350-line cap. A runless gate
//! is a standing grant raised at install time rather than by an event, so its
//! answer continues nothing and its whole job is to wake the parked delivery
//! — a different guarantee from the continuation suite's, proven differently.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{Decision, Inbox, Resolution};
use afd_core::clock::UnixMillis;
use afd_dragonfly::{ReadyIndex, SubscriptionHub};
use serde_json::{Value, json};

use crate::lane::{Lane, NOW_MS, WINDOW_MS, dead_queue, dragonfly_config};
use crate::tail_watch::{NOTE, OPERATOR, SUBSCRIBE_SETTLE, next_frame, ready_token};

/// A gate that held no run is answered, announced with no event, and
/// continues nothing.
///
/// The column is nullable for exactly this row — a standing grant raised at
/// install time — and an approval of it must decode, land, and say `null`
/// where a run's answer would name its event, rather than fail after the
/// row moved or continue a run that never was.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approval_of_a_gate_that_held_no_run_continues_nothing() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let runless = lane.seed_runless_gate(NOW_MS + WINDOW_MS).await;
    let outcome = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    let continued = match outcome {
        Resolution::Resolved(resolved) => resolved.continuation_event_id,
        Resolution::AlreadyResolved(_) | Resolution::NotFound => Some(String::new()),
    };
    assert_eq!(continued, None, "nothing to continue, and nothing invented");

    let frame = next_frame(&mut tail)
        .await
        .expect("the answer reaches the fleet's tail");
    assert_eq!(frame.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(frame.get("status"), Some(&json!("approved")));
    assert_eq!(frame.get("event_id"), Some(&Value::Null));
    assert_eq!(lane.status_of(&runless).await, "approved");
    assert_eq!(
        ready_token(&lane).await.as_deref(),
        Some(lane.fleet.as_str()),
        "a runless approval wakes the original parked delivery"
    );
}

/// A repeated answer to a runless gate still wakes the parked delivery.
///
/// The loser receives `AlreadyResolved`, but from the runner's point of view
/// the operator pressed the same wake button again. That must refresh Dragonfly too:
/// the original delivery is still the thing that will re-read the durable row.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_already_resolved_runless_gate_refreshes_readiness() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let runless = lane.seed_runless_gate(NOW_MS + WINDOW_MS).await;

    let first = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the first answer resolves the gate");
    assert!(matches!(first, Resolution::Resolved(_)));

    ReadyIndex::new(lane.queue.clone())
        .force_clear(lane.fleet.as_str())
        .await
        .expect("the test can clear the ready mark");
    assert_eq!(ready_token(&lane).await, None);

    let second = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the repeated answer reads the standing decision");
    assert!(matches!(second, Resolution::AlreadyResolved(_)));
    assert_eq!(
        ready_token(&lane).await.as_deref(),
        Some(lane.fleet.as_str()),
        "the already-resolved runless path wakes the parked delivery"
    );
}

/// A lost readiness refresh does not undo the durable answer.
///
/// The wake is best-effort: Dragonfly can be down after Postgres accepts the
/// person's decision. The resolve must still answer with the row's outcome so
/// a retry or sweeper can repair the readiness edge later.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_runless_gate_with_a_dead_ready_index_still_resolves() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let inbox = Inbox::new(
        lane.pool.clone(),
        dead_queue(),
        afd_admission::Admissions::for_tests(lane.pool.clone(), dead_queue()),
    );
    let runless = lane.seed_runless_gate(NOW_MS + WINDOW_MS).await;

    let outcome = inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("a lost ready mark does not reject the answer");
    assert!(matches!(outcome, Resolution::Resolved(_)));
    assert_eq!(lane.status_of(&runless).await, "approved");
}
