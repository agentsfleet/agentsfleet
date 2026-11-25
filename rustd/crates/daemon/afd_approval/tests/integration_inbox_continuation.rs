//! What an approved access card lands, and what it leaves untouched.
//!
//! The answer turn is a NEW turn carrying `resumes_turn_id`; the waiting row
//! is never reopened. Both halves are proven here over live Postgres: the row
//! that appears, the row that does not change, and the idempotence that keeps
//! a second answer from resuming the run twice.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use af_core::id::Uuid7;
use afd_approval::{Decision, Inbox, Resolution};
use afd_turns::{Budgets, Turns};

use crate::lane::{Lane, Window};

/// Who answers, when a test needs an operator.
const OPERATOR: &str = "human:fixture";
const OTHER_OPERATOR: &str = "human:somebody-else";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// An approval lands an answer turn that resumes the waiting run and tells it
/// the access was approved.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_approved_gate_lands_a_continuation_naming_what_it_resumes() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;

    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    let resolved = match outcome {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!("the first answer wins, got {other:?}"),
    };
    let continuation = resolved
        .continuation_turn_id
        .expect("an approval continues the run it unblocked");

    let actor = lane
        .event_column(continuation.as_str(), "actor")
        .await
        .expect("a continuation row carries an actor");
    let kind = lane
        .event_column(continuation.as_str(), "event_type")
        .await
        .expect("a continuation row carries a type");
    let resumes = lane
        .event_column(continuation.as_str(), "resumes_turn_id")
        .await;

    // The chain reads forward: the blocked row says what stopped, and this one
    // says what it resumed from — so a reader never joins back through the gate
    // table to reconstruct the history.
    let held = resolved
        .turn_id
        .as_ref()
        .map(Uuid7::as_str)
        .expect("a gate raised on a run names the event it held");
    assert_eq!(actor, format!("continuation:{held}"));
    assert_eq!(kind, "continuation");
    assert_eq!(resumes.as_deref(), Some(held));
    let body = lane
        .event_column(continuation.as_str(), "request_json")
        .await
        .expect("an answer turn carries a body");
    let answer: af_wire::turn::QuestionAnswer<'_> =
        serde_json::from_str(&body).expect("the body is a question's answer");
    assert_eq!(answer.message, "approved");
    assert_eq!(answer.asked_turn_id, held);
}

/// A re-answered gate does not continue the run twice.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_second_answer_does_not_continue_the_run_again() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;

    lane.inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");
    let after_first = lane.event_count().await;

    let second = lane
        .inbox
        .resolve(
            action.as_str(),
            Decision::Approved,
            OTHER_OPERATOR,
            NOTE,
            None,
        )
        .await
        .expect("the resolve must not fault");

    // The loser writes nothing: the `WHERE status = 'pending'` guard means the
    // second answer never reaches the continuation at all, so a retried resolve
    // restarts one run rather than two.
    assert!(matches!(second, Resolution::AlreadyResolved(_)));
    assert_eq!(
        after_first,
        lane.event_count().await,
        "answering twice continues the run once"
    );
}

/// The negative half — answering a gate leaves the waiting row alone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn resolving_a_gate_does_not_reopen_the_turn_that_waited() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    let waited = Uuid7::parse(&lane.gate_column(&action, "turn_id").await)
        .expect("the card names the turn that waits on it");
    let row_before = waiting_row(&lane, &waited).await;
    let before = lane.event_count().await;

    lane.inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    // The answer is a new row and the only new one; the waiting row keeps the
    // status and settlement it had, so the history still shows a person was
    // asked.
    assert_eq!(lane.event_count().await, before + 1, "one answer turn");
    assert_eq!(
        waiting_row(&lane, &waited).await,
        row_before,
        "answering a gate never rewrites the turn that waited"
    );
}

/// The waiting turn's status and settlement instant, as text.
async fn waiting_row(lane: &Lane, turn: &Uuid7) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, settled_at::text FROM thread.turns WHERE id = $1")
        .bind(turn)
        .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
        .await
        .expect("the waiting turn is readable")
}

/// A gate nobody answered inside its window is still answerable HOURS later,
/// and answering it still resumes the run.
///
/// The worked case is the one an operator actually meets: a fleet asks for a
/// secret at 10:00, nobody is at their desk, and a person approves it after
/// the card's window closed. That approval lands, and the run that waited
/// resumes.
///
/// # Nothing sweeps a gate
///
/// A sweep would take the gate when its window closed and make the later
/// approval answer `AlreadyResolved` instead of resuming anything. A read
/// reports `timed_out` past the deadline and the gate stays answerable.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_gate_answered_long_after_its_window_still_resumes_the_run() {
    let lane = Lane::isolated().await;

    // The deadline passed an hour ago on the database's clock — the clock a
    // sweeper would have taken the row on — and nothing swept it.
    let action = lane.seed_gate(Window::Lapsed).await;

    let outcome = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    let resolved = match outcome {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!(
            "a gate past its window is still PENDING and must be answerable, got {other:?}"
        ),
    };

    assert_eq!(
        lane.status_of(&action).await,
        "approved",
        "a late answer is an answer: nothing swept this gate out from under it"
    );

    // The row flipping is not the claim — the run restarting is. An approval
    // that changed a status and continued nothing is a person told "done" over
    // work that never resumed.
    let continuation = resolved
        .continuation_turn_id
        .expect("a late approval must still continue the run it unblocked");
    assert_eq!(
        lane.event_column(continuation.as_str(), "resumes_turn_id")
            .await,
        resolved.turn_id.map(String::from),
        "the continuation names the blocked event it resumes"
    );
}

/// An approval whose continuation could not be admitted is settled by the
/// retry: exactly one continuation lands, and a third answer lands none.
///
/// The first answer moves the row and then fails, so every later answer
/// takes the already-resolved path. Without the retry landing it, the run a
/// person approved would never restart.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_retry_lands_the_continuation_the_first_answer_could_not() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    let before = lane.event_count().await;
    refuse_continuation(&lane, &action, before).await;

    let retried = lane
        .inbox
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the retry settles the standing approval");
    let Resolution::AlreadyResolved(retried) = retried else {
        unreachable!("the first answer already moved the row, got {retried:?}");
    };
    let continuation = retried
        .continuation_turn_id
        .expect("the retry lands the continuation the first answer owed");
    assert_eq!(
        lane.event_count().await,
        before + 1,
        "exactly one continuation"
    );
    assert_eq!(
        lane.event_column(continuation.as_str(), "resumes_turn_id")
            .await,
        retried.turn_id.map(String::from),
        "the continuation names the blocked event it resumes"
    );

    let third = lane
        .inbox
        .resolve(
            action.as_str(),
            Decision::Approved,
            OTHER_OPERATOR,
            NOTE,
            None,
        )
        .await
        .expect("a third answer reads the standing decision");
    let Resolution::AlreadyResolved(third) = third else {
        unreachable!("the row stays answered, got {third:?}");
    };
    assert_eq!(
        third.continuation_turn_id.as_ref().map(Uuid7::as_str),
        Some(continuation.as_str()),
        "the third answer finds the continuation already landed"
    );
    assert_eq!(lane.event_count().await, before + 1, "and lands none");
}

async fn refuse_continuation(lane: &Lane, action: &Uuid7, before: i64) {
    // A ledger with no room refuses the continuation after the row moved.
    let full = Budgets {
        deployment_backlog: 0,
    };
    let refusing = Inbox::new(
        lane.pool.clone(),
        lane.bus.clone(),
        Turns::for_tests(lane.pool.clone(), lane.bus.clone()).with_budgets(full),
    );
    refusing
        .resolve(action.as_str(), Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect_err("the continuation's turn is refused");
    assert_eq!(lane.status_of(action).await, "approved");
    assert_eq!(lane.event_count().await, before, "nothing continued yet");
}
