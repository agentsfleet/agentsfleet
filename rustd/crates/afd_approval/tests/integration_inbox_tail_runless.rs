//! What the tail says for a gate that held no run.
//!
//! Split from `integration_inbox_tail.rs` at the 350-line cap. A runless gate
//! is a standing grant raised at install time rather than by an event, so its
//! answer continues nothing and its whole job is to wake the parked delivery
//! — a different guarantee from the continuation suite's, proven differently.
//! A denied run's retry is here too: it wakes a parked delivery the same way.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{Decision, Inbox, Resolution};
use afd_dragonfly::SubscriptionHub;
use serde_json::{Value, json};

use crate::lane::{Lane, Window, dead_queue, dragonfly_config};
use crate::tail_watch::{
    NOTE, OPERATOR, SUBSCRIBE_SETTLE, delivery_unparked, next_frame, park_delivery,
};

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
    park_delivery(&lane).await;

    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let runless = lane.seed_runless_gate(Window::Open).await;
    let outcome = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None)
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
    assert!(
        delivery_unparked(&lane).await,
        "a runless approval wakes the original parked delivery"
    );
}

/// A repeated answer to a runless gate still wakes the parked delivery.
///
/// The loser receives `AlreadyResolved` and the original delivery stays
/// available to the ledger probe.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_already_resolved_runless_gate_keeps_its_delivery_unparked() {
    let lane = Lane::isolated().await;
    park_delivery(&lane).await;
    let runless = lane.seed_runless_gate(Window::Open).await;

    let first = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the first answer resolves the gate");
    assert!(matches!(first, Resolution::Resolved(_)));

    assert!(
        delivery_unparked(&lane).await,
        "the first answer unparked durably"
    );

    let second = lane
        .inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the repeated answer reads the standing decision");
    assert!(matches!(second, Resolution::AlreadyResolved(_)));
    assert!(
        delivery_unparked(&lane).await,
        "the already-resolved runless path wakes the parked delivery"
    );
}

/// A repeated denial of a gate that held a run wakes the fleet again.
///
/// The denial left the run's delivery parked for the poll that ends it, and
/// the first answer's wake is best-effort. A retry after that wake was lost
/// is the one caller left to send it, so the already-resolved path wakes the
/// fleet for every answer that leaves a delivery parked, not only a runless
/// gate's.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_already_denied_gate_keeps_its_delivery_unparked() {
    let lane = Lane::isolated().await;
    park_delivery(&lane).await;
    let gate = lane.seed_gate(Window::Open).await;

    let first = lane
        .inbox
        .resolve(&gate, Decision::Denied, OPERATOR, NOTE, None)
        .await
        .expect("the first answer resolves the gate");
    assert!(matches!(first, Resolution::Resolved(_)));

    // Stands in for the first answer's wake never landing.
    assert!(
        delivery_unparked(&lane).await,
        "the first answer unparked durably"
    );

    let second = lane
        .inbox
        .resolve(&gate, Decision::Denied, OPERATOR, NOTE, None)
        .await
        .expect("the retry reads the standing decision");
    assert!(matches!(second, Resolution::AlreadyResolved(_)));
    assert!(
        delivery_unparked(&lane).await,
        "the retry wakes the parked delivery the denial left"
    );
}

/// A lost notification cannot undo the answer or leave its delivery parked.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_runless_gate_with_a_lost_notification_still_resolves() {
    let lane = Lane::isolated().await;
    park_delivery(&lane).await;
    let inbox = Inbox::new(
        lane.pool.clone(),
        dead_queue(),
        afd_admission::Admissions::for_tests(lane.pool.clone(), dead_queue()),
    );
    let runless = lane.seed_runless_gate(Window::Open).await;

    let outcome = inbox
        .resolve(&runless, Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("a lost notification does not reject the answer");
    assert!(matches!(outcome, Resolution::Resolved(_)));
    assert_eq!(lane.status_of(&runless).await, "approved");
    assert!(delivery_unparked(&lane).await);
}
