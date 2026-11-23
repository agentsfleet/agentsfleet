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

use af_core::id::Uuid7;
use afd_api_wire::approval::status;
use afd_approval::{Decision, Resolution};
use afd_turns::view::History;
use afd_turns::view::stop::{Stop, hold_thread, stop_on};
use sqlx::Connection as _;

use crate::lane::{Lane, Window};

const APPROVER: &str = "human:approver";

const NOTE: &str = "approved while stopping";

/// Whether any backend is waiting on `$1`'s locks.
const BLOCKING: &str =
    "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE $1 = ANY(pg_blocking_pids(pid)))";

/// A queued fleet turn parked behind its approval, as a run's park leaves it.
const PARKED_TURN: &str = "INSERT INTO thread.turns (id,thread_id,workspace_id,producer,producer_key,\
payload_digest,actor,kind,status,request_json,created_at,updated_at,parked_at) VALUES \
($1,$2,$3,'steer',$1,'fixture','steer:fixture','chat','queued','{}',now(),now(),now())";

/// Fleet delete's first hold (`afd_setup/src/creation/sql/lock.sql`).
const LOCK_FLEET: &str = "SELECT pg_backend_pid() FROM fleet.fleets WHERE id = $1 FOR UPDATE";

/// Fleet delete's close of its setup cards (`afd_approval/src/grant_sql/cancel_setup_cards.sql`).
const CLOSE_SETUP_CARDS: &str = "UPDATE fleet.approval_gates SET status = $2, resolved_by = 'delete', \
detail = '', resolved_at = now(), updated_at = now(), active_grant_id = NULL \
WHERE fleet_id = $1 AND resolved_at IS NULL AND gate_type = $3";

/// The gate kind fleet delete closes before it deletes the fleet.
const SETUP_CARD: &str = "integration_grant";

async fn parked_turn(lane: &Lane) -> Uuid7 {
    let id = afd_db::test_util::mint_id();
    sqlx::query(PARKED_TURN)
        .bind(&id)
        .bind(&lane.fleet)
        .bind(&lane.workspace)
        .execute(&mut *lane.pool.acquire().await.expect("a connection"))
        .await
        .expect("the parked turn inserts");
    id
}

/// Points `action`'s gate at `turn`, as a setup card when `card` is set.
async fn gate_turn(lane: &Lane, action: &Uuid7, turn: Option<&Uuid7>, card: bool) {
    sqlx::query(
        "UPDATE fleet.approval_gates SET turn_id = $2, \
         gate_type = CASE WHEN $3 THEN $4 ELSE gate_type END WHERE action_id = $1",
    )
    .bind(action)
    .bind(turn)
    .bind(card)
    .bind(SETUP_CARD)
    .execute(&mut *lane.pool.acquire().await.expect("a connection"))
    .await
    .expect("the gate moves");
}

async fn still_parked(lane: &Lane, turn: &Uuid7) -> bool {
    sqlx::query_scalar(
        "SELECT parked_at IS NOT NULL AND settled_at IS NULL FROM thread.turns WHERE id = $1",
    )
    .bind(turn)
    .fetch_one(&mut *lane.pool.acquire().await.expect("a connection"))
    .await
    .expect("the turn is readable")
}

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
    let gated = parked_turn(&lane).await;
    let other = parked_turn(&lane).await;
    gate_turn(&lane, &action, Some(&gated), false).await;

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
    assert!(
        still_parked(&lane, &other).await,
        "a resolution that lost unparks nothing"
    );
}

/// A resolution that waited on a thread holder still wins once the holder lets go:
/// after a holder that wrote the thread row and committed, as a renewal or a report
/// does, and after one that rolled back. Both parked turns of the fleet unpark.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_gate_resolution_that_waited_on_the_thread_wins_after_it() {
    for writes in [true, false] {
        let lane = Lane::isolated().await;
        let action = lane.seed_gate(Window::Open).await;
        let gated = parked_turn(&lane).await;
        let other = parked_turn(&lane).await;
        gate_turn(&lane, &action, Some(&gated), false).await;

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
        assert!(
            matches!(resolution, Resolution::Resolved(_)),
            "writes={writes}: the waiting resolution wins: {resolution:?}"
        );
        for turn in [&gated, &other] {
            assert!(
                !still_parked(&lane, turn).await,
                "writes={writes}: the turn unparks"
            );
        }
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_delete_finishes_against_a_resolution_and_a_stop() {
    for stopping in [false, true] {
        let lane = Lane::isolated().await;
        let action = lane.seed_gate(Window::Open).await;
        let gated = parked_turn(&lane).await;
        gate_turn(&lane, &action, stopping.then_some(&gated), true).await;

        let mut holder = lane.pool.acquire().await.expect("a connection");
        let mut deleting = holder.begin().await.expect("delete's transaction");
        let pid: i32 = sqlx::query_scalar(LOCK_FLEET)
            .bind(&lane.fleet)
            .fetch_one(&mut *deleting)
            .await
            .expect("delete holds the fleet");
        sqlx::query(CLOSE_SETUP_CARDS)
            .bind(&lane.fleet)
            .bind(status::ARCHIVED)
            .bind(SETUP_CARD)
            .execute(&mut *deleting)
            .await
            .expect("delete closes its setup cards");

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
