//! A gate's resolution against a Stop of its turn and against a fleet delete, on
//! live Postgres. Resolution and Stop each take the fleet and then its thread before
//! a gate or a turn, so they queue on the thread instead of locking a gate and a turn
//! in opposite orders, and both wait at the fleet while a delete that holds it
//! cascades to the gates and the thread.
//!
//! Marked `#[ignore]`; `make test-integration-rustd` runs them.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::pin::Pin;

use afd_api_wire::approval::status;
use afd_approval::{Decision, Resolution};
use afd_turns::Producer;
use afd_turns::view::History;
use afd_turns::view::stop::{Stop, hold_thread, stop_on};
use sqlx::Connection as _;

use crate::lane::{LOCK_FLEET, Lane, Window};

const APPROVER: &str = "human:approver";

const NOTE: &str = "approved while stopping";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// Whether `work` queues behind `pid` before it finishes; `work` keeps running.
async fn queues_behind<F: Future + Unpin>(lane: &Lane, pid: i32, work: &mut F) -> bool {
    let mut probe = lane.pool.acquire().await.expect("a connection");
    let waiter = async {
        loop {
            let queued: bool = sqlx::query_scalar(BLOCKING)
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .expect("inspect the lock queue");
            if queued {
                return;
            }
            tokio::task::yield_now().await;
        }
    };
    tokio::select! {
        _ = work => false,
        () = waiter => true,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_resolution_waits_for_a_stop_holding_the_thread() {
    let lane = Lane::isolated().await;
    let action = lane.seed_gate(Window::Open).await;
    let gated = lane.queued_turn().await;
    lane.gate_turn(&action, Some(&gated)).await;

    let mut holder = lane.pool.acquire().await.expect("a connection");
    let mut stopping = holder.begin().await.expect("Stop's transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *stopping)
        .await
        .expect("Stop's backend");
    hold_thread(&mut stopping, &lane.workspace, &lane.fleet)
        .await
        .expect("Stop holds the fleet and its thread");

    let (fleet, action_id) = (lane.fleet.to_string(), action.to_string());
    let mut resolve =
        Box::pin(
            lane.inbox
                .resolve(&action_id, Decision::Approved, APPROVER, NOTE, Some(&fleet)),
        );
    assert!(
        queues_behind(&lane, pid, &mut resolve).await,
        "the resolution queues behind the Stop holding the thread"
    );
    let stopped = stop_on(&mut stopping, &lane.workspace, &lane.fleet, &gated)
        .await
        .expect("Stop's statement")
        .expect("the turn exists");
    stopping.commit().await.expect("Stop commits");
    assert!(
        matches!(stopped.stop, Stop::Cancelled { approvals: 1, .. }),
        "Stop closes the turn's gate: {:?}",
        stopped.stop
    );

    let resolution = resolve.await.expect("the resolution answers");
    let stood = match resolution {
        Resolution::AlreadyResolved(stood) => Some(stood),
        _ => None,
    }
    .expect("Stop resolved the gate first");
    assert_eq!(stood.status, status::ARCHIVED);
    assert_eq!(
        lane.continuations(Producer::QuestionAnswer, &action).await,
        0,
        "a resolution that lost lands no answer turn"
    );
}

/// A resolution that waited on a thread holder still wins once the holder lets go:
/// after a holder that wrote the thread row and committed, as a renewal or a report
/// does, and after one that rolled back. The waiting turn gets its one answer turn.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_resolution_that_waited_on_the_thread_wins_after_it() {
    for writes in [true, false] {
        let lane = Lane::isolated().await;
        let action = lane.seed_gate(Window::Open).await;
        let gated = lane.queued_turn().await;
        lane.gate_turn(&action, Some(&gated)).await;

        let mut holder = lane.pool.acquire().await.expect("a connection");
        let mut holding = holder.begin().await.expect("the holder's transaction");
        let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *holding)
            .await
            .expect("the holder's backend");
        hold_thread(&mut holding, &lane.workspace, &lane.fleet)
            .await
            .expect("the holder takes the fleet and its thread");
        if writes {
            sqlx::query("UPDATE thread.threads SET updated_at = now() WHERE id = $1")
                .bind(&lane.fleet)
                .execute(&mut *holding)
                .await
                .expect("the holder writes the thread row");
        }

        let (fleet, action_id) = (lane.fleet.to_string(), action.to_string());
        let mut resolve = Box::pin(lane.inbox.resolve(
            &action_id,
            Decision::Approved,
            APPROVER,
            NOTE,
            Some(&fleet),
        ));
        assert!(
            queues_behind(&lane, pid, &mut resolve).await,
            "writes={writes}: the resolution queues behind the thread's holder"
        );
        if writes {
            holding.commit().await.expect("the holder commits");
        } else {
            holding.rollback().await.expect("the holder rolls back");
        }

        let resolution = resolve.await.expect("the resolution answers");
        let Resolution::Resolved(won) = resolution else {
            unreachable!("writes={writes}: the waiting resolution wins: {resolution:?}");
        };
        let answer = won
            .continuation_turn_id
            .expect("the approval answers the waiting turn");
        assert_eq!(
            lane.event_column(answer.as_str(), "resumes_turn_id").await,
            Some(gated.to_string()),
            "writes={writes}: the answer resumes the turn that waited"
        );
        assert_eq!(
            lane.continuations(Producer::QuestionAnswer, &action).await,
            1,
            "writes={writes}: one answer turn"
        );
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_delete_finishes_against_a_resolution_and_a_stop() {
    for stopping in [false, true] {
        let lane = Lane::isolated().await;
        let action = lane.seed_gate(Window::Open).await;
        let gated = lane.queued_turn().await;
        lane.gate_turn(&action, Some(&gated)).await;

        let mut holder = lane.pool.acquire().await.expect("a connection");
        let mut deleting = holder.begin().await.expect("delete's transaction");
        let pid: i32 = sqlx::query_scalar(LOCK_FLEET)
            .bind(&lane.fleet)
            .fetch_one(&mut *deleting)
            .await
            .expect("delete holds the fleet");

        let (fleet, action_id) = (lane.fleet.to_string(), action.to_string());
        let history = History::new(lane.pool.clone());
        let mut contender: Pin<Box<dyn Future<Output = Result<String, String>>>> = if stopping {
            Box::pin(async {
                history
                    .stop(&lane.workspace, &lane.fleet, &gated)
                    .await
                    .map(|stopped| format!("{stopped:?}"))
                    .map_err(|error| format!("{error:?}"))
            })
        } else {
            Box::pin(async {
                lane.inbox
                    .resolve(&action_id, Decision::Approved, APPROVER, NOTE, Some(&fleet))
                    .await
                    .map(|resolution| format!("{resolution:?}"))
                    .map_err(|error| format!("{error:?}"))
            })
        };
        assert!(
            queues_behind(&lane, pid, &mut contender).await,
            "stopping={stopping}: the contender queues behind the delete"
        );
        let deleted = async {
            afd_fleet_lifecycle::delete_locked(&mut deleting, &lane.workspace, &lane.fleet)
                .await
                .map_err(|error| format!("{error:?}"))?;
            deleting
                .commit()
                .await
                .map_err(|error| format!("{error:?}"))
        };
        let (contest, delete_outcome) = tokio::join!(contender, deleted);
        assert!(
            delete_outcome.is_ok(),
            "stopping={stopping}: the delete commits: {delete_outcome:?}"
        );
        let expected = if stopping { "None" } else { "NotFound" };
        assert_eq!(contest.as_deref(), Ok(expected), "stopping={stopping}");
    }
}
