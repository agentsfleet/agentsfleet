//! Terminal history settles a reassigned turn without executing another run.
#![cfg(feature = "test-util")]
#![expect(clippy::expect_used, reason = "test assertions")]

use crate::report_seed::{Held, held, held_in};
use crate::rows::{LAPSE, Row};
use crate::support::Fixtures;

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_completed_event_redelivery_does_not_execute() {
    let run = held().await;
    run.fixtures
        .end_event(&run.fleet, &run.turn_id, af_core::turn::status::PROCESSED)
        .await;
    age_original_assignment(&run).await;
    let fleet = run.fleet.clone();
    let answer = run
        .fixtures
        .plane()
        .assign_fleet_with(
            &mut afd_fleet::run::AssignRequest {
                runner: &run.spare,
                held: &[fleet],
                degraded: false,
                now: run.now,
                recovery: None,
            },
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |_run, _metadata| (),
        )
        .await
        .expect("poll");
    assert!(answer.is_none(), "{answer:?}");
    let mut connection = run.fixtures.database.acquire().await.expect("database");
    let settled: bool =
        sqlx::query_scalar("SELECT settled_at IS NOT NULL FROM thread.turns WHERE id = $1")
            .bind(&run.turn_id)
            .fetch_one(&mut *connection)
            .await
            .expect("queue state");
    let attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM thread.runs WHERE fleet_id = $1")
        .bind(&run.fleet)
        .fetch_one(&mut *connection)
        .await
        .expect("run count");
    assert!(settled, "the terminal turn leaves the queue");
    assert_eq!(attempts, 1, "no second execution");
    drop(connection);
    assert!(
        run.runs
            .select(&run.spare, &[])
            .await
            .expect("next poll")
            .is_none()
    );
    run.fixtures.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn terminal_redelivery_cannot_settle_after_its_assignment_is_superseded() {
    let run = held_in(Fixtures::create_isolated().await).await;
    run.fixtures
        .end_event(&run.fleet, &run.turn_id, af_core::turn::status::PROCESSED)
        .await;
    // The holder stops renewing, and the finished event is assigned again: the
    // assignment answers a terminal redelivery.
    run.fixtures
        .age(Row::Run(&run.run_key), "run_expires_at", LAPSE)
        .await;
    run.fixtures
        .age(Row::Thread(&run.fleet), "run_expires_at", LAPSE)
        .await;
    let again = crate::seed::select_fleet(&run.fixtures.runs(), &run.runner, &run.fleet)
        .await
        .expect("the finished turn is assigned again");
    assert_eq!(
        again.opened.received.delivery,
        afd_fleet::run::Delivery::Terminal
    );
    // A rival supersedes that assignment between the assignment and its settlement.
    sqlx::query(
        "WITH thread AS (UPDATE thread.threads SET fencing_token = fencing_token + 1 \
         WHERE fleet_id = $1 RETURNING fencing_token) \
         UPDATE thread.turns SET run_fence = (SELECT fencing_token FROM thread) WHERE id = $2",
    )
    .bind(&run.fleet)
    .bind(&run.turn_id)
    .execute(&mut *run.fixtures.database.acquire().await.expect("rival"))
    .await
    .expect("supersede between the assignment and settlement");
    let answer = run
        .fixtures
        .plane()
        .assign_selected(again.clone(), &run.runner, run.now)
        .await
        .expect("stale terminal completion is harmless");
    assert!(
        answer.is_none(),
        "the terminal event is never reissued: {answer:?}"
    );
    assert_superseded_assignment_retained(&run, again.fence.as_i64() + 1).await;
    run.fixtures
        .age(Row::Thread(&run.fleet), "run_expires_at", LAPSE)
        .await;
    let answer = reassign_terminal(&run).await;
    assert!(
        answer.is_none(),
        "the terminal event is never reissued: {answer:?}"
    );
    assert!(run.fixtures.turn_settled(&run.turn_id).await);
    run.fixtures.cleanup().await;
}

async fn assert_superseded_assignment_retained(run: &Held, fence: i64) {
    let state: (bool, Option<i64>, i64, i64) = sqlx::query_as(
        "SELECT a.settled_at IS NOT NULL,a.run_fence,s.fencing_token, \
         (SELECT count(*) FROM thread.runs WHERE fleet_id=a.thread_id) \
         FROM thread.turns a JOIN thread.threads s ON s.fleet_id=a.thread_id \
         WHERE a.id=$1 AND s.last_runner_id=$2",
    )
    .bind(&run.turn_id)
    .bind(&run.runner)
    .fetch_one(&mut *run.fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("superseded assignment still exists");
    assert_eq!(state, (false, Some(fence), fence, 1));
}

async fn age_original_assignment(run: &Held) {
    run.fixtures
        .age(Row::Run(&run.issued.run_id), "run_expires_at", LAPSE)
        .await;
    run.fixtures
        .age(Row::Thread(&run.fleet), "run_expires_at", LAPSE)
        .await;
}

async fn reassign_terminal(run: &Held) -> Option<()> {
    run.fixtures
        .plane()
        .assign_fleet_with(
            &mut afd_fleet::run::AssignRequest {
                runner: &run.spare,
                held: std::slice::from_ref(&run.fleet),
                degraded: false,
                now: run.now,
                recovery: None,
            },
            None,
            &mut af_observability::verb_span::VerbClock::untimed(),
            |_run, _metadata| (),
        )
        .await
        .expect("current assignee suppresses the terminal event")
}
