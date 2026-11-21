//! `M253_003` §2: an answer, early or late, lets the turn continue.
//!
//! Nothing expires a gate, so an answer after the deadline lands as one on
//! time does. A question's answer lands one continuation that carries the
//! answer and names the asking turn whose checkpoint its run resumes. Two
//! answers make one decision, and an archived fleet's gate takes none.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::time::Duration;

use afd_approval::{Decision, Resolution};
use afd_turns::Producer;
use tokio::sync::Barrier;

use crate::lane::{Lane, Window};
use crate::tail_watch::{NOTE, OPERATOR};

/// How long before the answer the deadline passed.
const LATE: Duration = Duration::from_hours(2);

/// Dimension 2.1: a policy gate timed out two hours ago, then approved. The
/// parked turn is released, and the approval lands the continuation the
/// next assignment takes.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_approval_after_the_deadline_releases_the_turn() {
    let lane = Lane::isolated().await;
    crate::tail_watch::park_delivery(&lane).await;
    let action = lane.seed_gate(Window::Open).await;
    lane.lapse_by(&action, LATE).await;

    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    let Resolution::Resolved(resolved) = outcome else {
        unreachable!("a late approval is an approval, got {outcome:?}");
    };
    assert_eq!(lane.status_of(&action).await, "approved");
    assert!(
        crate::tail_watch::delivery_unparked(&lane).await,
        "the parked turn is released"
    );
    assert!(resolved.continuation_turn_id.is_some());
    assert!(lane.continuation_is_unsettled(&action).await, "assignable");
}

/// Dimension 2.2: a `question` gate past its deadline answered "yes" lands
/// one continuation whose body carries the answer and the asking turn.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_question_answered_after_the_deadline_resumes_the_run() {
    let lane = Lane::isolated().await;
    let action = lane.seed_question(Window::Lapsed).await;

    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Answered, OPERATOR, "yes", None)
        .await
        .expect("the resolve must not fault");

    let Resolution::Resolved(resolved) = outcome else {
        unreachable!("a late answer is an answer, got {outcome:?}");
    };
    assert_eq!(lane.status_of(&action).await, "answered");
    let continuation = resolved
        .continuation_turn_id
        .expect("an answer continues the run");
    let asked = resolved.turn_id.expect("a question names its turn");
    assert_eq!(lane.continuations(Producer::QuestionAnswer, &action).await, 1);
    let body = lane
        .event_column(continuation.as_str(), "request_json")
        .await
        .expect("the continuation carries a body");
    let answer: af_wire::turn::QuestionAnswer<'_> =
        serde_json::from_str(&body).expect("the body is a question's answer");
    assert_eq!(answer.message, "yes");
    assert_eq!(answer.asked_turn_id, asked.as_str());
    assert_eq!(
        lane.event_column(continuation.as_str(), "resumes_turn_id")
            .await
            .as_deref(),
        Some(asked.as_str())
    );
}

/// Dimension 2.3: an answer and a denial race; one wins, the other is told
/// the standing decision, and at most one continuation exists.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn two_answers_to_one_gate_resolve_it_once() {
    let lane = Lane::isolated().await;
    let action = lane.seed_question(Window::Lapsed).await;
    let barrier = Arc::new(Barrier::new(2));
    let spawn = |decision: Decision, detail: &'static str| {
        let inbox = lane.inbox.clone();
        let barrier = Arc::clone(&barrier);
        let action = action.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            inbox
                .resolve(action.as_str(), decision, OPERATOR, detail, None)
                .await
        })
    };
    let answer = spawn(Decision::Answered, "yes");
    let deny = spawn(Decision::Denied, NOTE);
    let outcomes = [
        answer.await.expect("joins").expect("must not fault"),
        deny.await.expect("joins").expect("must not fault"),
    ];

    let won = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Resolution::Resolved(_)))
        .count();
    let lost = outcomes
        .iter()
        .filter(|outcome| matches!(outcome, Resolution::AlreadyResolved(_)))
        .count();
    assert_eq!((won, lost), (1, 1), "one decision, and one told of it");
    assert!(lane.continuations(Producer::QuestionAnswer, &action).await <= 1);
}

/// Dimension 2.4: an approval after the fleet is archived is refused, and no
/// turn is written.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_answer_to_an_archived_fleets_gate_is_refused() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    let before = lane.event_count().await;
    lane.archive_fleet().await;

    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    assert_eq!(outcome, Resolution::FleetArchived);
    assert_eq!(lane.status_of(&action).await, "pending");
    assert_eq!(lane.event_count().await, before, "no turn written");
}
