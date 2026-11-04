//! A missing database result cannot strand the only pool permit or the task.

use std::{error::Error as _, time::Duration};

use afd_admission::Admissions;
use afd_core::{error_code, id::Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::{Db, config::DbRole};
use afd_fleet::{
    lease::Leases,
    task::{TaskLifecycle as _, TaskScope, Tasks},
};
use tokio::time::{Instant, timeout};

use super::fixture::{Case, exercise};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_missing_query_result_hits_the_client_deadline_and_releases_its_pool_permit() {
    exercise(deadline).await;
}

async fn deadline(case: &Case) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let accepted = tasks
        .bind(case.lease(), &case.plan("readable-after-outage", 1))
        .await
        .expect("accepted work");
    let task = &accepted.items.first().expect("accepted task").task_id;
    let before = tasks.read(scope, task).await.expect("baseline snapshot");
    let (database, tasks) = single_pool(case).await;
    let admin = case.held.fixtures.open(DbRole::Migrator).await;
    let mut connection = admin.acquire().await.expect("owned blocker connection");
    let mut blocker = sqlx::Connection::begin(&mut *connection)
        .await
        .expect("blocker transaction");
    sqlx::query("SELECT pg_advisory_xact_lock(872365)")
        .execute(&mut *blocker)
        .await
        .expect("hold read lock");
    sqlx::raw_sql(include_str!("fixtures/stall_read.sql"))
        .execute(&mut *admin.acquire().await.expect("isolated policy connection"))
        .await
        .expect("query remains blocked after server cancellation");

    missing_result(&tasks, scope, task).await;
    available_connection(&database).await;
    blocker
        .rollback()
        .await
        .expect("release the owned read blocker");
    drop(connection);
    sqlx::raw_sql(include_str!("fixtures/restore_read.sql"))
        .execute(&mut *admin.acquire().await.expect("policy cleanup connection"))
        .await
        .expect("remove the isolated failure injection");
    assert_eq!(
        tasks
            .read(scope, task)
            .await
            .expect("same replica recovers"),
        before
    );
    assert_eq!(case.counts().await, (1, 1, 1));
    database.close().await;
    admin.close().await;
}

async fn available_connection(database: &Db) {
    let mut healthy = timeout(Duration::from_secs(2), database.acquire())
        .await
        .expect("the only permit is available before the blocked server finishes")
        .expect("a fresh connection is usable");
    let one: i32 = sqlx::query_scalar("SELECT 1")
        .fetch_one(&mut *healthy)
        .await
        .expect("fresh connection query");
    assert_eq!(one, 1);
}

async fn single_pool(case: &Case) -> (Db, Tasks) {
    let database = case
        .held
        .fixtures
        .open_with(
            DbRole::Api,
            &[
                ("DATABASE_POOL_SIZE_API", "1"),
                ("DATABASE_MIN_POOL_SIZE_API", "0"),
            ],
        )
        .await;
    let tasks = Tasks::new(
        Leases::new(
            database.clone(),
            case.held.fixtures.queue().clone(),
            Entropy::new(),
        ),
        Admissions::new(
            database.clone(),
            case.held.fixtures.queue().clone(),
            Entropy::new(),
        ),
    );
    (database, tasks)
}

async fn missing_result(tasks: &Tasks, scope: &TaskScope, task: &Uuid7) {
    let started = Instant::now();
    let failure = timeout(Duration::from_secs(8), tasks.read(scope, task))
        .await
        .expect("outer guard bounds a broken client deadline")
        .expect_err("a lost result is unavailable");
    assert_eq!(failure.code(), error_code::INTERNAL_DB_UNAVAILABLE);
    assert!(failure.is_datastore_unavailable());
    assert!(
        failure
            .source()
            .expect("client deadline cause")
            .is::<tokio::time::error::Elapsed>()
    );
    assert!(
        started.elapsed() >= Duration::from_secs(5),
        "the server's three-second cancellation is not this boundary"
    );
}
