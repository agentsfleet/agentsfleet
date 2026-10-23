//! Observe parallel SQL from 100 clients, then release them to race one admission.

use std::sync::Arc;

use afd_core::id::Uuid7;
use afd_db::config::DbRole;
use afd_fleet::lease::{Acquired, Leases};
use sqlx::Connection as _;
use tokio::{sync::Barrier, task::JoinHandle};

use super::{WITHIN, enqueue, stamps};
use crate::{seed::seeded_parts, support::Fixtures};

const CLAIMERS: usize = 100;
const MIN_PARALLEL: i64 = 4;
const PAUSE_CLAIMS: &str = "CREATE FUNCTION fleet.pause_claim() RETURNS trigger
    LANGUAGE plpgsql AS $$ BEGIN
      PERFORM pg_advisory_xact_lock_shared(oid::bigint)
      FROM pg_database WHERE datname = current_database();
      RETURN NEW;
    END $$;
    CREATE TRIGGER pause_claim BEFORE INSERT ON runner.fleet_slots
    FOR EACH ROW EXECUTE FUNCTION fleet.pause_claim()";
const HOLD_CLAIMS: &str = "SELECT pg_backend_pid() FROM pg_database,
    LATERAL pg_advisory_xact_lock(oid::bigint) WHERE datname = current_database()";
const BLOCKED: &str = "SELECT count(*) FROM pg_stat_activity
    WHERE datname = current_database() AND wait_event = 'advisory'
    AND $1 = ANY(pg_blocking_pids(pid))";

type Claimer = JoinHandle<afd_fleet::Result<Option<Acquired>>>;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_concurrent_claims_lease_one_row() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, workspace, _, runners) = seeded_parts::<2>(&fixtures).await;
    let event = enqueue(&fixtures, &fleet, &workspace).await;
    let replica = Leases::new(
        fixtures.open(DbRole::Api).await,
        crate::queue::unreachable(),
        afd_crypto::entropy::Entropy::new(),
    );
    let replicas = [fixtures.leases_with_dead_queue(), replica];
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("owner");
    let mut observer = owner.acquire().await.expect("reserved observer");
    sqlx::raw_sql(PAUSE_CLAIMS)
        .execute(&mut *connection)
        .await
        .expect("private claim trigger");
    let mut blocker = connection.begin().await.expect("hold claims");
    let pid: i32 = sqlx::query_scalar(HOLD_CLAIMS)
        .fetch_one(&mut *blocker)
        .await
        .expect("exclusive barrier");
    let tasks = start_claimers(&replicas, &runners);
    let parallel = parallel_claims(&mut observer, pid).await;
    assert!(parallel >= MIN_PARALLEL, "parallel SQL: {parallel}");
    blocker.rollback().await.expect("release claim barrier");
    let winners = winners(tasks).await;
    assert_eq!(winners.len(), 1);
    assert_eq!(winners.first().expect("one winner").event_id, event);
    assert_eq!(stamps(&fixtures, &event).await, (Some(1), Some(1)));
    drop((connection, observer, replicas));
    owner.close().await;
    fixtures.cleanup().await;
}

fn start_claimers(replicas: &[Leases; 2], runners: &[Uuid7; 2]) -> Vec<Claimer> {
    let barrier = Arc::new(Barrier::new(CLAIMERS));
    replicas
        .iter()
        .zip(runners)
        .cycle()
        .take(CLAIMERS)
        .map(|(leases, runner)| {
            let leases = leases.clone();
            let runner = runner.clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                leases.select(&runner, &[]).await
            })
        })
        .collect()
}

async fn parallel_claims(observer: &mut sqlx::PgConnection, pid: i32) -> i64 {
    tokio::time::timeout(WITHIN, async {
        loop {
            let count: i64 = sqlx::query_scalar(BLOCKED)
                .bind(pid)
                .fetch_one(&mut *observer)
                .await
                .expect("observe claim waits");
            if count >= MIN_PARALLEL {
                return count;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("multiple claimers reach Postgres together")
}

async fn winners(tasks: Vec<Claimer>) -> Vec<Acquired> {
    let mut winners = Vec::new();
    for task in tasks {
        if let Some(acquired) = tokio::time::timeout(WITHIN, task)
            .await
            .expect("bounded contention")
            .expect("claimer joins")
            .expect("claim query")
        {
            winners.push(acquired);
        }
    }
    winners
}
