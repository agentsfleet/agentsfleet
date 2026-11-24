//! A gate answer committed during a lock wait must prevent the delayed park.

use af_core::id::Uuid7;
use afd_approval::{Decision, Inbox, Resolution};
use afd_turns::Turns;
use sqlx::Connection as _;

use super::{WITHIN, enqueue, next};
use crate::{seed::seeded_parts, support::Fixtures};

const HOLD: &str = "SELECT pg_backend_pid() FROM thread.threads
    WHERE fleet_id = $1 FOR UPDATE";
const BLOCKED: &str = "SELECT EXISTS (SELECT 1 FROM pg_stat_activity
    WHERE $1 = ANY(pg_blocking_pids(pid)))";
// Waiters behind `$1` and behind those: a second row-lock waiter queues on the first
// waiter's tuple lock, so Postgres names the first waiter, not the holder.
const WAITERS: &str = "WITH first AS (SELECT pid FROM pg_stat_activity
      WHERE $1 = ANY(pg_blocking_pids(pid)))
    SELECT count(*) FROM pg_stat_activity a
    WHERE $1 = ANY(pg_blocking_pids(a.pid))
       OR EXISTS (SELECT 1 FROM first f WHERE f.pid = ANY(pg_blocking_pids(a.pid)))";
const GATE: &str = "INSERT INTO fleet.approval_gates
    (id, fleet_id, workspace_id, action_id, tool_name, action_name, gate_type,
     proposed_action, evidence, blast_radius, expires_at, status, detail,
     created_at, updated_at, turn_id)
    VALUES ($1, $2, $3, $1, 'git', 'push', 'repository_write',
      'open a pull request', '{}'::jsonb, 'one repository', now()+interval '1 hour',
      $4, '', now(), now(), NULL)";

// Value: protects=a park and an answer that meet on the thread leave no stranded turn;
// fails_when=the answer is lost behind the park, or the park outlives the answer;
// why_new=the resolver takes the thread first, so it queues behind the park;
// seam=none; no_claim=an answer that commits before the park starts, covered by the
// park's own pending-gate check.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn an_answer_during_the_park_lock_wait_cannot_strand_the_turn() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner]) = seeded_parts::<1>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let acquired = next(&runs, &runner).await.expect("assignment");
    let action = seed_gate(&fixtures, &fleet, &workspace).await;
    let mut connection = fixtures.database.acquire().await.expect("blocker");
    let mut blocker = connection
        .begin()
        .await
        .expect("hold thread before turn lock");
    let pid: i32 = sqlx::query_scalar(HOLD)
        .bind(&fleet)
        .fetch_one(&mut *blocker)
        .await
        .expect("fleet thread locked");
    let (parked, ()) = tokio::join!(
        runs.park(&acquired.fleet_id, &event, acquired.fence),
        answer_while_blocked(&fixtures, &action, pid, blocker)
    );
    parked.expect("delayed park completes");
    let is_parked: bool =
        sqlx::query_scalar("SELECT parked_at IS NOT NULL FROM thread.turns WHERE id=$1")
            .bind(&event)
            .fetch_one(&mut *connection)
            .await
            .expect("park stamp");
    assert!(!is_parked, "the committed answer prevents a stale park");
    assert_eq!(
        next(&runs, &runner)
            .await
            .expect("answered head runs")
            .turn_id,
        event
    );
    drop((connection, runs));
    fixtures.cleanup().await;
}

pub(super) async fn seed_gate(fixtures: &Fixtures, fleet: &Uuid7, workspace: &Uuid7) -> Uuid7 {
    let action = afd_db::test_util::mint_id();
    sqlx::query(GATE)
        .bind(&action)
        .bind(fleet)
        .bind(workspace)
        .bind(afd_api_wire::approval::status::PENDING)
        .execute(&mut *fixtures.database.acquire().await.expect("seed connection"))
        .await
        .expect("pending runless gate");
    action
}

#[tokio::test]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn a_stale_assignment_cannot_park_or_release_its_replacement() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, [runner, rival]) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let runs = fixtures.runs_with_dead_bus();
    let old = next(&runs, &runner).await.expect("first assignment");
    seed_gate(&fixtures, &fleet, &workspace).await;
    fixtures
        .age(
            crate::rows::Row::Thread(&fleet),
            "run_expires_at",
            crate::rows::LAPSE,
        )
        .await;
    let current = next(&runs, &rival).await.expect("replacement assignment");
    assert!(current.fence.as_i64() > old.fence.as_i64());
    runs.park(&old.fleet_id, &event, old.fence)
        .await
        .expect("stale park is harmless");
    let state: (bool, Option<i64>, i64) = sqlx::query_as(
        "SELECT a.parked_at IS NOT NULL, a.run_fence, s.fencing_token \
         FROM thread.turns a JOIN thread.threads s ON s.fleet_id=a.thread_id \
         WHERE a.id=$1 AND s.last_runner_id=$2",
    )
    .bind(&event)
    .bind(&rival)
    .fetch_one(&mut *fixtures.database.acquire().await.expect("observer"))
    .await
    .expect("the replacement still owns the thread");
    assert_eq!(
        state,
        (false, Some(current.fence.as_i64()), current.fence.as_i64())
    );
    runs.park(&current.fleet_id, &event, current.fence)
        .await
        .expect("the current holder can park");
    assert!(fixtures.thread_is_free(&fleet).await);
    drop(runs);
    fixtures.cleanup().await;
}

async fn answer_while_blocked(
    fixtures: &Fixtures,
    action: &Uuid7,
    pid: i32,
    blocker: sqlx::Transaction<'_, sqlx::Postgres>,
) {
    wait_for_park(fixtures, pid).await;
    let bus = afd_bus::test_util::unreachable();
    let inbox = Inbox::new(
        fixtures.database.clone(),
        bus.clone(),
        Turns::for_tests(fixtures.database.clone(), bus),
    );
    let answer = inbox.resolve(
        action.as_str(),
        Decision::Approved,
        "fixture:human",
        "",
        None,
    );
    // The answer takes the thread first too, so it queues behind the park; the thread's
    // holder lets go once both wait, the park commits, and the answer unparks the turn.
    let release = async {
        wait_for_waiters(fixtures, pid, 2).await;
        blocker.rollback().await.expect("release park");
    };
    let (answer, ()) = tokio::join!(answer, release);
    let answer = answer.expect("the answer commits after the park");
    assert!(matches!(answer, Resolution::Resolved(_)));
}

async fn wait_for_waiters(fixtures: &Fixtures, pid: i32, waiters: i64) {
    let mut observer = fixtures.database.acquire().await.expect("observer");
    tokio::time::timeout(WITHIN, async {
        loop {
            let queued: i64 = sqlx::query_scalar(WAITERS)
                .bind(pid)
                .fetch_one(&mut *observer)
                .await
                .expect("observe the thread's queue");
            if queued >= waiters {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the park and the answer both queue on the thread");
}

pub(super) async fn wait_for_park(fixtures: &Fixtures, pid: i32) {
    let mut observer = fixtures.database.acquire().await.expect("observer");
    tokio::time::timeout(WITHIN, async {
        loop {
            let blocked: bool = sqlx::query_scalar(BLOCKED)
                .bind(pid)
                .fetch_one(&mut *observer)
                .await
                .expect("observe park lock");
            if blocked {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("park reaches the held turn");
}
