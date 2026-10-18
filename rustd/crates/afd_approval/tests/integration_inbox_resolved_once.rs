//! Dimension 4.6 — a resolved gate is resolved once.
//!
//! `resolved_at` is the gate's one pending marker: NULL while it waits, stamped
//! by the database clock when it is answered, and every answering statement is
//! guarded on it. The schema's trigger is the second wall — a resolved row
//! refuses every change but one more spend — so a statement that forgot the
//! guard still could not re-answer a gate. Both are proven here, against the
//! real table, because both are properties of what Postgres does.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api_wire::approval::status;
use afd_approval::{Decision, Resolution};
use jiff::Timestamp;

use crate::lane::{Lane, NOW, Window};

/// Who answers first.
const OPERATOR: &str = "human:fixture";

/// Who answers second, too late.
const LATECOMER: &str = "human:latecomer";

/// The note an operator leaves.
const NOTE: &str = "looks right";

/// When `action`'s gate was resolved, straight from the table.
async fn resolved_at(lane: &Lane, action: &str) -> Option<Timestamp> {
    sqlx::query_scalar::<_, Option<jiff_sqlx::Timestamp>>(
        "SELECT resolved_at FROM fleet.approval_gates WHERE action_id = $1",
    )
    .bind(action)
    .fetch_one(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await
    .expect("resolved_at is readable")
    .map(jiff_sqlx::Timestamp::to_jiff)
}

/// A gate answered once keeps the instant and the decider of that answer.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_resolution_happens_once() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    assert_eq!(
        resolved_at(&lane, &action).await,
        None,
        "a pending gate carries no resolution instant"
    );

    let first = match lane
        .inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault")
    {
        Resolution::Resolved(resolved) => resolved,
        other => unreachable!("an open gate is answerable, got {other:?}"),
    };
    let stamped = resolved_at(&lane, &action)
        .await
        .expect("an answer stamps the gate");
    assert_eq!(
        first.resolved_at, stamped,
        "the resolve reports the instant the database wrote"
    );
    assert!(
        stamped > NOW,
        "the stamp is the database's clock, not the fixture's"
    );

    let second = lane
        .inbox
        .resolve(&action, Decision::Denied, LATECOMER, NOTE, None)
        .await
        .expect("the second resolve must not fault");
    let standing = match second {
        Resolution::AlreadyResolved(standing) => standing,
        other => unreachable!("a resolved gate is not answerable again, got {other:?}"),
    };
    assert_eq!(standing.resolved_by, OPERATOR);
    assert_eq!(standing.resolved_at, stamped);
    assert_eq!(
        resolved_at(&lane, &action).await,
        Some(stamped),
        "a second answer does not re-stamp the first"
    );
}

/// The schema refuses to rewrite a resolved gate, guard or no guard.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_resolved_gate_refuses_an_unguarded_rewrite() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    lane.inbox
        .resolve(&action, Decision::Approved, OPERATOR, NOTE, None)
        .await
        .expect("the resolve must not fault");

    // The statement a buggy resolver would run: no `resolved_at IS NULL`.
    let rewrite = sqlx::query(
        "UPDATE fleet.approval_gates
            SET status = $2, resolved_by = $3, resolved_at = now(), updated_at = now()
          WHERE action_id = $1",
    )
    .bind(&action)
    .bind(status::DENIED)
    .bind(LATECOMER)
    .execute(&mut *lane.pool.acquire().await.expect("the lane must answer"))
    .await;

    assert!(
        rewrite.is_err(),
        "the guard trigger refuses a change to a resolved gate"
    );
    assert_eq!(lane.status_of(&action).await, status::APPROVED);
}
