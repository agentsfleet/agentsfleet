//! Two runners race one queued turn, 100 callers between them. The first to
//! lock the fleet's thread is paused inside its assignment; every other
//! assignment skips the locked thread and finishes with nothing while it
//! waits, and the paused one is the only winner, at fence 1.

use std::sync::Arc;

use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_fleet::run::{Acquired, Runs};
use sqlx::Connection as _;
use tokio::{sync::Barrier, task::JoinHandle};

use super::{WITHIN, enqueue, reassignment::issue, stamps};
use crate::{rows::Row, seed::seeded_parts, support::Fixtures};

const RUNNERS: usize = 100;
const PAUSE_ASSIGNMENTS: &str = "CREATE FUNCTION fleet.pause_assignment() RETURNS trigger
    LANGUAGE plpgsql AS $$ BEGIN
      PERFORM pg_advisory_xact_lock_shared(oid::bigint)
      FROM pg_database WHERE datname = current_database();
      RETURN NEW;
    END $$;
    CREATE TRIGGER pause_assignment BEFORE INSERT ON thread.threads
    FOR EACH ROW EXECUTE FUNCTION fleet.pause_assignment()";
const HOLD_ASSIGNMENTS: &str = "SELECT pg_backend_pid() FROM pg_database,
    LATERAL pg_advisory_xact_lock(oid::bigint) WHERE datname = current_database()";
const BLOCKED: &str = "SELECT count(*) FROM pg_stat_activity
    WHERE datname = current_database() AND wait_event = 'advisory'
    AND $1 = ANY(pg_blocking_pids(pid))";

type Contender = JoinHandle<afd_fleet::Result<Option<Acquired>>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn two_assignments_on_one_thread_raise_its_fence_once_each() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, tenant, runners) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let replica = Runs::new(
        fixtures.open(DbRole::Api).await,
        afd_bus::test_util::unreachable(),
        af_crypto::entropy::Entropy::new(),
    );
    let replicas = [fixtures.runs_with_dead_bus(), replica];
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    let mut observer = owner.acquire().await.expect("reserved observer");
    sqlx::raw_sql(PAUSE_ASSIGNMENTS)
        .execute(&mut *connection)
        .await
        .expect("private assignment trigger");
    let mut blocker = connection.begin().await.expect("hold assignments");
    let pid: i32 = sqlx::query_scalar(HOLD_ASSIGNMENTS)
        .fetch_one(&mut *blocker)
        .await
        .expect("exclusive barrier");
    let tasks = start_contenders(&replicas, &runners);
    assert_eq!(
        paused_assignments(&mut observer, pid).await,
        1,
        "one assignment holds the thread"
    );
    others_finish(&tasks).await;
    blocker
        .rollback()
        .await
        .expect("release assignment barrier");
    let winners = winners(tasks).await;
    assert_eq!(winners.len(), 1, "the other runner gets nothing");
    let winner = winners.first().expect("one winner");
    assert_eq!(winner.turn_id, event);
    assert_eq!(winner.fence.as_i64(), 1);
    assert_eq!(stamps(&fixtures, &event).await, (Some(1), Some(1)));
    let holder = fixtures
        .text(Row::Thread(&fleet), "last_runner_id")
        .await
        .and_then(|id| Uuid7::parse(&id).ok())
        .expect("the assignment names its runner on the thread");
    assert!(runners.contains(&holder), "one of the two runners won");
    let run = issue(&fixtures, &replicas[0], winner, &holder, &tenant).await;
    assert_eq!(
        runs_at(&fixtures, &fleet).await,
        vec![(run, event.clone(), 1)],
        "one run, at fence 1"
    );
    drop((connection, observer, replicas));
    owner.close().await;
    fixtures.cleanup().await;
}

fn start_contenders(replicas: &[Runs; 2], runners: &[Uuid7; 2]) -> Vec<Contender> {
    let barrier = Arc::new(Barrier::new(RUNNERS));
    replicas
        .iter()
        .zip(runners)
        .cycle()
        .take(RUNNERS)
        .map(|(runs, runner)| {
            let runs = runs.clone();
            let runner = runner.clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                runs.select(&runner, &[]).await
            })
        })
        .collect()
}

/// How many assignments wait at the paused insert: the one that locked the thread.
async fn paused_assignments(observer: &mut sqlx::PgConnection, pid: i32) -> i64 {
    tokio::time::timeout(WITHIN, async {
        loop {
            let count: i64 = sqlx::query_scalar(BLOCKED)
                .bind(pid)
                .fetch_one(&mut *observer)
                .await
                .expect("observe assignment waits");
            if count > 0 {
                return count;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("a contender reaches the paused insert")
}

/// Every assignment but the paused one finishes while it waits: they skip the
/// locked thread rather than queue on it.
async fn others_finish(tasks: &[Contender]) {
    tokio::time::timeout(WITHIN, async {
        while tasks.iter().filter(|task| task.is_finished()).count() < RUNNERS - 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("the losing assignments do not wait on the locked thread");
}

async fn winners(tasks: Vec<Contender>) -> Vec<Acquired> {
    let mut winners = Vec::new();
    for task in tasks {
        if let Some(acquired) = tokio::time::timeout(WITHIN, task)
            .await
            .expect("bounded contention")
            .expect("contender joins")
            .expect("assignment query")
        {
            winners.push(acquired);
        }
    }
    winners
}

/// Every run on `fleet`'s thread: its id, turn and fence.
async fn runs_at(fixtures: &Fixtures, fleet: &Uuid7) -> Vec<(Uuid7, Uuid7, i64)> {
    sqlx::query_as("SELECT id, turn_id, fencing_token FROM thread.runs WHERE fleet_id = $1")
        .bind(fleet)
        .fetch_all(
            &mut *fixtures
                .database
                .acquire()
                .await
                .expect("runtime connection"),
        )
        .await
        .expect("the thread's runs")
}
