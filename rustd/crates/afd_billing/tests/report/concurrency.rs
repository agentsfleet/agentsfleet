use super::fixture::{self, Fixture, Line};
use afd_billing::tenant::Billing;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, DbRole, Migrator};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Barrier;
use tokio::task::JoinSet;

const CLIENTS: usize = 100;
const CONNECTIONS: usize = 8;
const ROUNDS: usize = 3;
const DEADLINE: Duration = Duration::from_secs(5);
const OBSERVATION_INTERVAL: Duration = Duration::from_millis(10);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres: make test-integration-rustd"]
async fn simultaneous_report_clients_overlap_and_return_every_connection() {
    // The table-lock observation must not block unrelated tests' ledger writes.
    let database = TestDatabase::create().await;
    let admin = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&admin).await.unwrap();
    admin.close().await;
    let f = Fixture::open(&database).await;
    fixture::seed(&f.admin, &[Line::setup(&f.tenant, 17)]).await;
    let api = database
        .open(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "8"),
                ("DATABASE_MIN_POOL_SIZE_API", "8"),
            ],
        )
        .await;
    assert_eq!(api.warm(DEADLINE).await as usize, CONNECTIONS);
    for round in 0..ROUNDS {
        run_round(&f, &api, round).await;
    }
    let mut returned = Vec::new();
    for _ in 0..CONNECTIONS {
        returned.push(
            tokio::time::timeout(DEADLINE, api.acquire())
                .await
                .unwrap()
                .unwrap(),
        );
    }
    assert_eq!(returned.len(), CONNECTIONS);
    drop(returned);
    assert_eq!(fixture::ledger(&f.admin, &f.tenant).await, (1, 17, 1));
    api.close().await;
    f.close().await;
    database.cleanup().await;
}

async fn run_round(f: &Fixture, api: &Db, round: usize) {
    let mut lock = f.admin.acquire().await.unwrap();
    let mut lock = sqlx::Connection::begin(&mut *lock).await.unwrap();
    sqlx::query("LOCK TABLE billing.usage_lines IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let barrier = Arc::new(Barrier::new(CLIENTS + 1));
    let mut jobs = JoinSet::new();
    for client in 0..CLIENTS {
        let barrier = Arc::clone(&barrier);
        let billing = Billing::new(api.clone());
        let tenant = f.tenant.clone();
        jobs.spawn(async move {
            barrier.wait().await;
            if (client + round).is_multiple_of(2) {
                tokio::task::yield_now().await;
            }
            billing.report(&tenant, &fixture::request(10)).await
        });
    }
    barrier.wait().await;
    let overlapping = tokio::time::timeout(DEADLINE, observe(&f.admin))
        .await
        .unwrap();
    assert_eq!(
        usize::try_from(overlapping).expect("nonnegative connection count fits usize"),
        CONNECTIONS,
        "real queries overlap behind the table lock"
    );
    lock.commit().await.unwrap();
    let mut completed = 0;
    while let Some(result) = jobs.join_next().await {
        let report = result.unwrap().unwrap();
        assert_eq!(
            (
                fixture::summary(&report).charged_nanos,
                report.activities.len()
            ),
            (17, 1)
        );
        completed += 1;
    }
    assert_eq!(completed, CLIENTS);
    println!("report_round={round} clients={completed} simultaneous_database_reads={overlapping}");
}

async fn observe(admin: &Db) -> i64 {
    loop {
        let waiting: i64 = sqlx::query_scalar("SELECT COUNT(*)::bigint FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'WITH scoped AS%'")
            .fetch_one(&mut *admin.acquire().await.unwrap()).await.unwrap();
        if usize::try_from(waiting).expect("nonnegative connection count fits usize") == CONNECTIONS
        {
            return waiting;
        }
        tokio::time::sleep(OBSERVATION_INTERVAL).await;
    }
}
