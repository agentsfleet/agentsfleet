//! Each gate kind against each way a gate ends that no other file pairs it with: a
//! denial, a Stop of a turn, and a fleet delete. A tool gate and a question hang from
//! the turn that raised them; a runless gate and a setup card hang from no turn.
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
use crate::tail_watch::{NOTE, OPERATOR, delivery_unparked, park_delivery};

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
    for producer in [Producer::QuestionAnswer, Producer::GateContinuation] {
        assert_eq!(
            lane.continuations(producer, &question).await,
            0,
            "no {} turn",
            producer.as_str()
        );
    }
}

/// A denied runless gate continues nothing and wakes the delivery it parked.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_denied_runless_gate_continues_nothing_and_wakes_its_delivery() {
    let lane = Lane::isolated().await;
    park_delivery(&lane).await;
    let runless = lane.seed_runless_gate(Window::Open).await;

    let resolved = deny(&lane, &runless).await;

    assert_eq!(
        resolved.continuation_turn_id, None,
        "a denial continues nothing"
    );
    assert_eq!(lane.status_of(&runless).await, status::DENIED);
    assert!(
        delivery_unparked(&lane).await,
        "a runless denial wakes the parked delivery"
    );
}

/// A Stop closes the question its turn asked and continues nothing; a runless gate and
/// a setup card, which hang from no turn, stay pending.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stop_closes_its_turns_question_and_leaves_turnless_gates_pending() {
    let lane = Lane::isolated().await;
    let asking = lane.parked_turn().await;
    let question = lane.seed_question(Window::Open).await;
    lane.gate_turn(&question, Some(&asking), false).await;
    let runless = lane.seed_runless_gate(Window::Open).await;
    let card = lane.seed_runless_gate(Window::Open).await;
    lane.gate_turn(&card, None, true).await;

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
    for gate in [&runless, &card] {
        assert_eq!(lane.status_of(gate).await, status::PENDING);
    }
}

/// A fleet delete takes its tool gates and questions with it, and a later answer finds
/// no gate.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_fleet_delete_takes_its_tool_gates_and_questions_with_it() {
    let lane = Lane::isolated().await;
    let tool = lane.seed_gate(Window::Open).await;
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
    for (gate, decision) in [(&tool, Decision::Approved), (&question, Decision::Answered)] {
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
