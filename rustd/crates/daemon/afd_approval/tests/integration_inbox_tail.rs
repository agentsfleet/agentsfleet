//! What the inbox tells the fleet's live tail.
//!
//! A decision and a sweep both move a row and then say so on the fleet's
//! live tail, `Subject::ThreadItems`, count included, so a console watching the
//! fleet shows how many answers it is still owed without a read. Read back
//! through the bus's own subscribe on that subject, the one the console's tail
//! reads, so a publisher that drifted from its reader fails here.
//!
//! This file holds the ANNOUNCEMENT concern: that a frame goes out, what it
//! counts, and what happens when the bus will not take it. The answer turn is
//! the sibling suite's. A denied question answers nothing, so its frame is the
//! first and only one a tail hears.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_approval::{Decision, Inbox, Resolution};
use serde_json::json;

use crate::lane::{Lane, Window};
use crate::tail_watch::{NOTE, OPERATOR, next_frame, tail};

/// A decision is announced on the fleet's live tail, count included.
///
/// The row moved first and the frame names it, so a console reacting to the
/// frame reads the answer it describes. The count rides the frame so the
/// console shows how many approvals still wait without a read of its own —
/// which is why a second gate is seeded: the count must say ONE, not zero.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_decision_is_announced_on_the_fleets_live_tail() {
    let lane = Lane::isolated().await;

    let mut tail = tail(&lane).await;

    let answered = lane.seed_question(Window::Open).await;
    let _still_waiting = lane.seed_question(Window::Open).await;
    lane.inbox
        .resolve(answered.as_str(), Decision::Denied, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    assert_decision_frame(&lane, &answered, &mut tail).await;
}

async fn assert_decision_frame(
    lane: &Lane,
    answered: &af_core::id::Uuid7,
    tail: &mut afd_bus::Subscription,
) {
    let frame = next_frame(tail)
        .await
        .expect("the decision reaches the fleet's tail");
    assert_eq!(frame.get("kind"), Some(&json!("gate_resolved")));
    assert_eq!(frame.get("status"), Some(&json!("denied")));
    assert_eq!(frame.get("resolved_by"), Some(&json!(OPERATOR)));
    assert_eq!(
        frame.get("turn_id"),
        Some(&json!(lane.gate_column(answered, "turn_id").await)),
        "the frame names the event the gate held"
    );
    assert_eq!(
        frame.get("gate_id"),
        Some(&json!(lane.gate_column(answered, "id").await)),
        "and the gate itself"
    );
    assert_eq!(
        frame.get("pending_approvals"),
        Some(&json!(1)),
        "the sibling gate still waits, and the frame says so"
    );
}

/// A re-raised action's rows are answered together, and counted together.
///
/// `action_id` carries no unique constraint: an action raised twice holds a
/// second pending row, and one decision moves both. The count on
/// the frame is read on the statement's own snapshot, where both rows still
/// stand as pending — so it must leave out everything the statement moved,
/// not the one row the caller happened to read back.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_re_raised_actions_rows_are_counted_out_together() {
    let lane = Lane::isolated().await;

    let mut tail = tail(&lane).await;

    let re_raised = lane.seed_question(Window::Open).await;
    lane.seed_question_for(&re_raised, Window::Open).await;
    let _sibling = lane.seed_question(Window::Open).await;
    lane.inbox
        .resolve(re_raised.as_str(), Decision::Denied, OPERATOR, NOTE, None)
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

/// A bus that will not take the frame does not fail the decision.
///
/// The row moved over live Postgres before the announcement ran, and a
/// person's answer must not be reported as failed because nobody could be told
/// about it. A denied question proves the announcement alone; an approved card
/// also lands its answer turn, which
/// `an_approval_with_a_lost_notification_still_commits_its_continuation` proves.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_bus_that_will_not_take_the_frame_does_not_fail_the_decision() {
    let lane = Lane::isolated().await;
    let inbox = Inbox::new(
        lane.pool.clone(),
        afd_bus::test_util::unreachable(),
        afd_turns::Turns::for_tests(lane.pool.clone(), afd_bus::test_util::unreachable()),
    );
    let action = lane.seed_question(Window::Open).await;

    let outcome = inbox
        .resolve(action.as_str(), Decision::Denied, OPERATOR, NOTE, None)
        .await
        .expect("the decision stands whatever the bus does");
    assert!(
        matches!(outcome, Resolution::Resolved(_)),
        "the operator decided; the lost announcement is the log's, not theirs"
    );
    assert_eq!(lane.status_of(&action).await, "denied");
}
