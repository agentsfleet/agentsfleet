//! Dimension 6.4 — what the inbox tells the fleet's live tail.
//!
//! A decision and a sweep both move a row and then say so on
//! `fleet:{id}:activity`, count included, so a console watching the fleet
//! shows how many answers it is still owed without a read. Proven against the
//! production subscriber for the reason the daemon's activity suite is: a raw
//! `SUBSCRIBE` would agree with a publisher that had drifted from its reader.
//!
//! This file holds the ANNOUNCEMENT concern: that a frame goes out, what it
//! counts, and what happens when the queue will not take it. The continuation
//! and runless concerns are siblings.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{Decision, Inbox, Resolution};
use afd_core::clock::UnixMillis;
use afd_dragonfly::SubscriptionHub;
use serde_json::json;

use crate::lane::{Lane, NOW_MS, WINDOW_MS, dead_queue, dragonfly_config, sweeper_exclusive};
use crate::tail_watch::{NOTE, OPERATOR, SUBSCRIBE_SETTLE, SWEEPER, next_frame};

/// A decision is announced on the fleet's live tail, count included.
///
/// The row moved first and the frame names it, so a console reacting to the
/// frame reads the answer it describes. The count rides the frame so the
/// console shows how many approvals still wait without a read of its own —
/// which is why a second gate is seeded: the count must say ONE, not zero.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_decision_is_announced_on_the_fleets_live_tail() {
    let _sweeper = sweeper_exclusive().await;
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let answered = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let _still_waiting = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    lane.inbox
        .resolve(&answered, Decision::Denied, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");

    let frame = next_frame(&mut tail)
        .await
        .expect("the decision reaches the fleet's tail");
    assert_eq!(frame.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(frame.get("status"), Some(&json!("denied")));
    assert_eq!(frame.get("resolved_by"), Some(&json!(OPERATOR)));
    assert_eq!(
        frame.get("event_id"),
        Some(&json!(lane.gate_column(&answered, "event_id").await)),
        "the frame names the event the gate held"
    );
    assert_eq!(
        frame.get("gate_id"),
        Some(&json!(lane.gate_column(&answered, "id").await)),
        "and the gate itself"
    );
    assert_eq!(
        frame.get("pending_approvals"),
        Some(&json!(1)),
        "the sibling gate still waits, and the frame says so"
    );

    // The sweeper announces what it takes, the same way: two seeded lapsed,
    // so this sweep takes both — reading the fleet's counters once for the
    // pair — and the sibling above stays.
    let lapsed = lane.seed_gate(NOW_MS - 1).await;
    let lapsed_too = lane.seed_gate(NOW_MS - 1).await;
    lane.inbox
        .expire(now)
        .await
        .expect("the sweep must not fault");
    let swept = next_frame(&mut tail)
        .await
        .expect("the sweep reaches the fleet's tail");
    let swept_too = next_frame(&mut tail)
        .await
        .expect("the second swept gate reaches the tail too");
    assert_eq!(swept_too.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(
        swept_too.get("events_processed"),
        swept.get("events_processed"),
        "one fleet, one read: both frames carry the same snapshot"
    );
    assert_eq!(swept.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(swept.get("status"), Some(&json!("timed_out")));
    assert_eq!(swept.get("resolved_by"), Some(&json!(SWEEPER)));
    let counters = afd_events::fleet_counters(&lane.pool, lane.fleet.as_str())
        .await
        .expect("the counters read back");
    assert_eq!(
        swept.get("events_processed"),
        Some(&json!(counters.events_processed)),
        "the sweeper's frame carries where the fleet stands"
    );
    let mut swept_events = [
        swept.get("event_id").cloned(),
        swept_too.get("event_id").cloned(),
    ];
    swept_events.sort_by_key(|value| value.as_ref().map(ToString::to_string));
    let mut seeded_events = [
        Some(json!(lane.gate_column(&lapsed, "event_id").await)),
        Some(json!(lane.gate_column(&lapsed_too, "event_id").await)),
    ];
    seeded_events.sort_by_key(|value| value.as_ref().map(ToString::to_string));
    assert_eq!(
        swept_events, seeded_events,
        "both lapsed gates are announced"
    );
    assert_eq!(swept.get("pending_approvals"), Some(&json!(1)));
}

/// A re-raised action's rows are answered together, and counted together.
///
/// `action_id` carries no unique constraint: a park that re-raises an action
/// writes a second pending row, and one decision moves both. The count on
/// the frame is read on the statement's own snapshot, where both rows still
/// stand as pending — so it must leave out everything the statement moved,
/// not the one row the caller happened to read back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_re_raised_actions_rows_are_counted_out_together() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let hub = SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's Dragonfly accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let re_raised = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    lane.seed_gate_for(&re_raised, NOW_MS + WINDOW_MS).await;
    let _sibling = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    lane.inbox
        .resolve(&re_raised, Decision::Denied, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");

    let frame = next_frame(&mut tail)
        .await
        .expect("the decision reaches the fleet's tail");
    assert_eq!(frame.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(
        frame.get("pending_approvals"),
        Some(&json!(1)),
        "both rows of the answered action are out; the sibling alone waits"
    );
}

/// A queue that will not take the frame does not fail the decision.
///
/// The row moved over live Postgres before the announcement ran, and a
/// person's answer must not be reported as failed because nobody could be told
/// about it. The denial arm is the one to prove: an approval also appends a
/// continuation to the queue, which a dead queue refuses for its own reason.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_queue_that_will_not_take_the_frame_does_not_fail_the_decision() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);
    let inbox = Inbox::new(
        lane.pool.clone(),
        dead_queue(),
        afd_admission::Admissions::for_tests(lane.pool.clone(), dead_queue()),
    );
    let action = lane.seed_gate(NOW_MS + WINDOW_MS).await;

    let outcome = inbox
        .resolve(&action, Decision::Denied, OPERATOR, NOTE, None, now)
        .await
        .expect("the decision stands whatever the queue does");
    assert!(
        matches!(outcome, Resolution::Resolved(_)),
        "the operator decided; the lost announcement is the log's, not theirs"
    );
    assert_eq!(lane.status_of(&action).await, "denied");
}
