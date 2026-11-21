//! `M253_003` §1: a run that asked the person settles its turn `waiting` and
//! raises a `question` gate on it, in the report's own transaction.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_fleet::run::{Committed, Terminal, Verdict};

use crate::report_commit::report;
use crate::report_seed::held;
use crate::rows::Row;

/// The question the run asked.
const QUESTION: &str = "Ship it?";

/// Dimension 1.1: a run ending on `ask_user` leaves one `question` gate for
/// its turn, pending and naming the question, and the turn `waiting`. The
/// checkpoint at the asking call is the runner's, proven by its own suite.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_ask_user_ending_writes_a_question_gate() {
    let held = held().await;
    let runs = held.fixtures.runs_with_dead_bus();
    let run = runs
        .load_for_report(&held.issued.run_id, &held.runner)
        .await
        .expect("the run load must reach the datastore")
        .expect("the seeded run belongs to the seeded runner");
    let mut waiting = report(
        &held.issued.run_id,
        &held.runner,
        &run,
        (&held.run_key, &held.pricing),
        QUESTION,
        held.now,
    );
    waiting.outcome = Terminal {
        verdict: Verdict::Waiting,
        ..waiting.outcome
    };

    let committed = runs
        .commit_report(waiting)
        .await
        .expect("the report must reach the datastore");
    assert!(matches!(committed, Committed::Settled { .. }));

    assert_eq!(
        held.fixtures
            .text(Row::Turn(&held.turn_id), "status")
            .await
            .as_deref(),
        Some(af_core::turn::status::WAITING)
    );
    let gates: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT gate_type, status, proposed_action FROM fleet.approval_gates \
         WHERE fleet_id = $1 AND turn_id = $2",
    )
    .bind(&held.fleet)
    .bind(&held.turn_id)
    .fetch_all(&mut *held.fixtures.database.acquire().await.expect("database"))
    .await
    .expect("the gates read");
    assert_eq!(
        gates,
        vec![(
            afd_api_wire::approval::GATE_TYPE_QUESTION.to_owned(),
            afd_api_wire::approval::status::PENDING.to_owned(),
            QUESTION.to_owned(),
        )]
    );

    held.fixtures.cleanup().await;
}
