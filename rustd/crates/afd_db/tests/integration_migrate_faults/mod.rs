//! What the migrator does when a slot or its ledger misbehaves, against a live
//! database.
//!
//! `integration_migrate/mod.rs` proves what a migrate does when it works. These
//! prove what it does when it does not: a failed slot applies nothing and is
//! never recorded, a broken ledger write fails as itself, and a ledger naming a
//! slot this binary does not know refuses the run.
//!
//! Every fault here is injected by breaking the database out from under the
//! code — dropping the table it is about to write, killing the session holding
//! the lock — because that is the only honest way to reach a branch written for
//! a datastore that misbehaves. Nothing here mocks sqlx.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migrate::{MigrationLock, RetryPolicy};
use afd_db::migration::Migration;
use afd_db::test_util::TestDatabase;
use sqlx::Row as _;

/// A migration whose SQL ends inside a string literal.
///
/// Postgres must refuse the whole batch and leave no partial schema change.
const UNTERMINATED: &[Migration] = &[Migration::for_test(
    9001,
    "9001_unterminated_literal.sql",
    "CREATE TABLE public.never_created (id int); SELECT 'unterminated",
)];

/// A migration that removes the table the migrator is about to record it in.
///
/// Contrived on purpose, and the shape is not: it is a schema change that
/// invalidates the ledger's own storage mid-transaction, which is exactly what
/// a migration touching the `audit` schema could do by accident.
const DROPS_THE_LEDGER: &[Migration] = &[Migration::for_test(
    9002,
    "9002_drops_the_ledger.sql",
    "DROP TABLE audit._sqlx_migrations",
)];

/// A retry policy that gives up quickly; nothing here contends for the lock.
const IMPATIENT: RetryPolicy = RetryPolicy::new(3, Duration::from_millis(20));

/// Whether sqlx recorded `version` as applied.
async fn recorded(connection: &mut sqlx::PgConnection, version: i64) -> bool {
    sqlx::query("SELECT count(*) = 1 FROM audit._sqlx_migrations WHERE version = $1")
        .bind(version)
        .fetch_one(connection)
        .await
        .unwrap()
        .get(0)
}

/// A malformed SQL batch applies nothing and is never recorded.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_malformed_migration_applies_nothing() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let error = Migrator::new()
        .with_migrations(UNTERMINATED)
        .run(&db)
        .await
        .expect_err("a migration that ends inside a literal must be refused");

    assert!(error.is_migration_failed(), "got {error}");
    assert!(
        error.to_string().contains("9001"),
        "names the slot: {error}"
    );
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.unwrap();
    assert!(
        !recorded(&mut connection, 9001).await,
        "a failed slot is never recorded"
    );
    let created: bool = sqlx::query("SELECT to_regclass('public.never_created') IS NOT NULL")
        .fetch_one(&mut *connection)
        .await
        .unwrap()
        .get(0);
    assert!(
        !created,
        "the batch is refused whole; no statement in it may apply"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A migration that invalidates the ledger mid-transaction fails as a ledger
/// error and applies nothing: the slot and its ledger row commit together.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_migration_that_breaks_its_own_ledger_rolls_back() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let error = Migrator::new()
        .with_migrations(DROPS_THE_LEDGER)
        .run(&db)
        .await
        .expect_err("an insert into a dropped ledger must fail the run");
    assert!(
        error.is_query(),
        "the ledger write failed, not the slot: {error}"
    );

    let mut connection = db.acquire().await.unwrap();
    let ledger_exists: bool =
        sqlx::query("SELECT to_regclass('audit._sqlx_migrations') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await
            .unwrap()
            .get(0);
    assert!(
        ledger_exists,
        "the transaction rolled back, so the ledger is still there"
    );
    assert!(
        !recorded(&mut connection, 9002).await,
        "nothing was recorded"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A ledger naming a slot this binary does not know refuses the run, and the
/// unknown row is left alone: an older image rolled onto a newer database stops.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_ledger_ahead_of_this_binary_refuses_the_run() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let mut connection = db.acquire().await.unwrap();
    sqlx::query(
        "INSERT INTO audit._sqlx_migrations \
             (version, description, success, checksum, execution_time) \
         VALUES (99999, 'from a newer binary', TRUE, '\\x00', 0)",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    drop(connection);

    let error = Migrator::new()
        .run(&db)
        .await
        .expect_err("a slot this binary does not know must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert!(
        error.to_string().contains("99999"),
        "names the slot: {error}"
    );
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.unwrap();
    assert!(
        recorded(&mut connection, 99_999).await,
        "the unknown row survives"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// Releasing the migration lock over a killed session is survived, not raised.
///
/// `release` takes `self` and returns nothing, because by the time it runs the
/// migrate is over and there is no caller left to tell. The session being gone
/// is also the lock being gone — Postgres drops advisory locks with the backend
/// that holds them — so the unlock failing means the work it guards is already
/// done. Raising here would turn a successful migrate into a failed boot.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_releasing_a_lock_over_a_killed_session_is_survived() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let connection = db.acquire().await.unwrap();
    let guard = MigrationLock::acquire(connection, IMPATIENT)
        .await
        .expect("an uncontended lock");

    // The holder is found by the lock it holds rather than by asking the guard
    // for its session: that keeps the fixture out of the lock's internals, and
    // the per-test database is what makes "the advisory lock in this database"
    // unambiguous.
    let mut bystander = db.acquire().await.unwrap();
    let killed: i64 = sqlx::query(
        "SELECT count(pg_terminate_backend(pid))
           FROM pg_locks
          WHERE locktype = 'advisory'
            AND granted
            AND database = (SELECT oid FROM pg_database WHERE datname = current_database())
            AND pid <> pg_backend_pid()",
    )
    .fetch_one(&mut *bystander)
    .await
    .unwrap()
    .get(0);
    assert_eq!(killed, 1, "exactly one session holds this database's lock");
    drop(bystander);

    // Returns `()`. Reaching the next line is the assertion.
    guard.release().await;

    db.close().await;
    database.cleanup().await;
}

/// A benign slot, so a run has a ledger to leave behind.
const BENIGN: &[Migration] = &[Migration::for_test(
    9003,
    "9003_benign.sql",
    "CREATE TABLE public.migrate_fault_benign (id int)",
)];

/// A ledger row sqlx marked unfinished refuses the next run: a slot that died
/// partway is not one a later binary may build on.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_unfinished_ledger_row_refuses_the_run() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .with_migrations(BENIGN)
        .run(&db)
        .await
        .expect("the benign slot applies");

    let mut connection = db.acquire().await.unwrap();
    sqlx::query(
        "INSERT INTO audit._sqlx_migrations \
             (version, description, success, checksum, execution_time) \
         VALUES (9004, 'died partway', FALSE, '\\x00', 0)",
    )
    .execute(&mut *connection)
    .await
    .unwrap();
    drop(connection);

    let error = Migrator::new()
        .with_migrations(BENIGN)
        .run(&db)
        .await
        .expect_err("an unfinished slot must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    db.close().await;
    database.cleanup().await;
}

/// A lock poll over a session that died fails as the query it is, at once: a
/// dropped connection is not contention, so the bound is not waited out.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_lock_poll_over_a_killed_session_fails_without_retrying() {
    /// A bound whose first retry alone would outlast the assertion below.
    const PATIENT: RetryPolicy = RetryPolicy::new(5, Duration::from_secs(2));

    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let mut doomed = db.acquire().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *doomed)
        .await
        .unwrap();
    let mut bystander = db.acquire().await.unwrap();
    let killed: bool = sqlx::query_scalar("SELECT pg_terminate_backend($1)")
        .bind(pid)
        .fetch_one(&mut *bystander)
        .await
        .unwrap();
    assert!(killed, "the doomed session was terminated");
    drop(bystander);

    let started = std::time::Instant::now();
    let error = MigrationLock::acquire(doomed, PATIENT)
        .await
        .expect_err("a poll on a dead session cannot take the lock");

    assert!(
        error.is_query(),
        "a dead session is a query failure: {error}"
    );
    assert!(!error.is_migration_refused(), "and not contention: {error}");
    assert!(
        started.elapsed() < PATIENT.budget() / 5,
        "the poll was not retried"
    );

    db.close().await;
    database.cleanup().await;
}
