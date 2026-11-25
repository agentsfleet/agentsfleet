//! Each gate kind against each way a gate ends that no other file pairs it with: a
//! denial, a Stop of a turn, and a fleet delete. A question and an access card each
//! hang from the turn whose run waits on them.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::approval::status;
use afd_approval::{Decision, Resolution, Resolved};
use afd_turns::Producer;
use afd_turns::view::History;
use afd_turns::view::stop::Stop;
use sqlx::Connection as _;

use crate::lane::{LOCK_FLEET, Lane, Window};
use crate::tail_watch::{NOTE, OPERATOR};

/// The first answer to `action`, which wins.
async fn deny(lane: &Lane, action: &af_core::id::Uuid7) -> Resolved {
    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Denied, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");
    match outcome {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!("the first answer wins, got {other:?}"),
    }
}

/// A denied question is closed and continues nothing: no answer reaches the run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_denied_question_continues_nothing() {
    let lane = Lane::isolated().await;
    let question = lane.seed_question(Window::Open).await;

    let resolved = deny(&lane, &question).await;

    assert_eq!(
        resolved.continuation_turn_id, None,
        "a denial continues nothing"
    );
    assert_eq!(lane.status_of(&question).await, status::DENIED);
    assert_eq!(
        lane.continuations(Producer::QuestionAnswer, &question)
            .await,
        0,
        "no answer turn"
    );
}

/// A Stop closes the question its turn asked and continues nothing; an access card
/// another turn waits on stays pending.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stop_closes_its_turns_question_and_leaves_another_turns_card_pending() {
    let lane = Lane::isolated().await;
    let asking = lane.queued_turn().await;
    let question = lane.seed_question(Window::Open).await;
    lane.gate_turn(&question, Some(&asking)).await;
    let card = lane.seed_gate(Window::Open).await;

    let stopped = History::new(lane.pool.clone())
        .stop(&lane.workspace, &lane.fleet, &asking)
        .await
        .expect("the Stop commits")
        .expect("the turn exists");

    assert!(
        matches!(stopped.stop, Stop::Cancelled { approvals: 1, .. }),
        "the Stop closes its turn's question and nothing else: {:?}",
        stopped.stop
    );
    assert_eq!(lane.status_of(&question).await, status::ARCHIVED);
    assert_eq!(lane.gate_column(&question, "resolved_by").await, "stop");
    assert_eq!(
        lane.continuations(Producer::QuestionAnswer, &question)
            .await,
        0,
        "a stopped question is answered by nobody"
    );
    assert_eq!(lane.status_of(&card).await, status::PENDING);
}

/// A fleet delete takes its access cards and questions with it, and a later answer
/// finds no gate.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_delete_takes_its_cards_and_questions_with_it() {
    let lane = Lane::isolated().await;
    let card = lane.seed_gate(Window::Open).await;
    let question = lane.seed_question(Window::Open).await;

    let mut connection = lane.pool.acquire().await.expect("a connection");
    let mut deleting = connection.begin().await.expect("delete's transaction");
    sqlx::query(LOCK_FLEET)
        .bind(&lane.fleet)
        .fetch_one(&mut *deleting)
        .await
        .expect("delete holds the fleet");
    afd_fleet_lifecycle::delete_locked(&mut deleting, &lane.workspace, &lane.fleet)
        .await
        .expect("the delete runs");
    deleting.commit().await.expect("the delete commits");

    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM fleet.approval_gates WHERE fleet_id = $1")
            .bind(&lane.fleet)
            .fetch_one(&mut *lane.pool.acquire().await.expect("a connection"))
            .await
            .expect("the gates read");
    assert_eq!(left, 0, "the delete cascades to every gate");
    let fleet = lane.fleet.to_string();
    for (gate, decision) in [(&card, Decision::Approved), (&question, Decision::Answered)] {
        let outcome = lane
            .inbox
            .resolve(gate.as_str(), decision, OPERATOR, NOTE, Some(&fleet))
            .await
            .expect("the resolve answers");
        assert!(
            matches!(outcome, Resolution::NotFound),
            "{}: {outcome:?}",
            decision.as_str()
        );
    }
}
