//! Dimension 6.4 — what the inbox tells the fleet's live tail.
//!
//! A decision and a sweep both move a row and then say so on
//! `fleet:{id}:activity`, count included, so a console watching the fleet
//! shows how many answers it is still owed without a read. Proven against the
//! production subscriber for the reason the daemon's activity suite is: a raw
//! `SUBSCRIBE` would agree with a publisher that had drifted from its reader.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_approval::{Decision, Inbox, Resolution};
use afd_core::clock::UnixMillis;
use afd_redis::hub::Received;
use afd_redis::{Redis, Subscription, SubscriptionHub};
use serde_json::{Value, json};

use crate::lane::{Lane, NOW_MS, WINDOW_MS, redis_config, sweeper_exclusive};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// The resolver a swept gate records, mirrored from the store.
const SWEEPER: &str = "system:approval_gate_sweeper";

/// How long a published frame is given to reach the subscriber.
const FRAME_DEADLINE: Duration = Duration::from_secs(5);

/// How long the hub's pump is given to register the subscription with Redis.
const SUBSCRIBE_SETTLE: Duration = Duration::from_millis(250);

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

    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
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

    // The sweeper announces what it takes, the same way: seeded lapsed, so
    // this sweep takes it and the sibling above stays.
    let lapsed = lane.seed_gate(NOW_MS - 1).await;
    lane.inbox
        .expire(now)
        .await
        .expect("the sweep must not fault");
    let swept = next_frame(&mut tail)
        .await
        .expect("the sweep reaches the fleet's tail");
    assert_eq!(swept.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(swept.get("status"), Some(&json!("timed_out")));
    assert_eq!(swept.get("resolved_by"), Some(&json!(SWEEPER)));
    assert_eq!(
        swept.get("event_id"),
        Some(&json!(lane.gate_column(&lapsed, "event_id").await))
    );
    assert_eq!(swept.get("pending_approvals"), Some(&json!(1)));
}

/// An approval opens the continued run on the tail before it announces the
/// answer.
///
/// The common operator path, and the one with an ordering to prove: the
/// continuation row is inserted by the resolve, so the runner's pull finds it
/// already there and announces nothing — the resolve is the one writer that
/// can open it on the tail. A watcher hears `event_received` for the
/// continuation, then `gate_resolved` for the answer, and the count on the
/// answer says the sibling still waits.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approval_opens_the_continued_run_before_it_announces_the_answer() {
    let lane = Lane::isolated().await;
    let now = UnixMillis::from_millis(NOW_MS);

    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
    let mut tail = hub.subscribe(&format!("fleet:{}:activity", lane.fleet));
    tokio::time::sleep(SUBSCRIBE_SETTLE).await;

    let approved = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let _still_waiting = lane.seed_gate(NOW_MS + WINDOW_MS).await;
    let outcome = lane
        .inbox
        .resolve(&approved, Decision::Approved, OPERATOR, NOTE, None, now)
        .await
        .expect("the resolve must not fault");
    let continuation = match outcome {
        Resolution::Resolved(resolved) => resolved.continuation_event_id,
        Resolution::AlreadyResolved(_) | Resolution::NotFound => None,
    }
    .expect("a pending gate is this caller's to answer, and an approval continues its run");

    let opened = next_frame(&mut tail)
        .await
        .expect("the continued run opens on the fleet's tail");
    assert_eq!(opened.get("kind"), Some(&json!("event_received")));
    assert_eq!(opened.get("event_id"), Some(&json!(continuation)));
    assert_eq!(opened.get("event_type"), Some(&json!("continuation")));
    assert_eq!(
        opened.get("actor"),
        Some(&json!(format!(
            "continuation:{}",
            lane.gate_column(&approved, "event_id").await
        )))
    );

    let answered = next_frame(&mut tail)
        .await
        .expect("then the answer reaches the tail");
    assert_eq!(answered.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(answered.get("status"), Some(&json!("approved")));
    assert_eq!(
        answered.get("pending_approvals"),
        Some(&json!(1)),
        "the sibling gate still waits, and the frame says so"
    );
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

    let hub = SubscriptionHub::start(redis_config())
        .await
        .expect("the lane's Redis accepts a subscriber");
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
    let deaf = Redis::unreachable(&redis_config()).expect("a lazy handle needs no socket");
    let inbox = Inbox::new(lane.pool.clone(), deaf);
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

/// The next frame on the tail, as JSON, or `None` if none arrives in time.
async fn next_frame(tail: &mut Subscription) -> Option<Value> {
    let received = tokio::time::timeout(FRAME_DEADLINE, tail.recv())
        .await
        .ok()?;
    let Received::Message(message) = received.expect("the subscription stays live") else {
        return None;
    };
    serde_json::from_str(&message.payload).ok()
}
