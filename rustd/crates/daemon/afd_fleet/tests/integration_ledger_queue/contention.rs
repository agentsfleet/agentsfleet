//! 100 clients race one turn. The first to lock the fleet's thread is paused
//! inside its assignment; every other assignment skips the locked thread and finishes
//! with nothing while it waits, and the paused one is the only winner.

use std::sync::Arc;

use af_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_fleet::run::{Acquired, Runs};
use sqlx::Connection as _;
use tokio::{sync::Barrier, task::JoinHandle};

use super::{WITHIN, enqueue, stamps};
use crate::{seed::seeded_parts, support::Fixtures};

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
async fn test_concurrent_assignments_run_one_row() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, runners) = seeded_parts::<2>(&fixtures).await;
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
    assert_eq!(winners.len(), 1);
    assert_eq!(winners.first().expect("one winner").turn_id, event);
    assert_eq!(stamps(&fixtures, &event).await, (Some(1), Some(1)));
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
