//! A runtime error after successful SQL must roll back the entire migration.
//!
//! Unlike malformed SQL, the missing relation is discovered after the first
//! statement executes. Only the real transaction can undo that earlier DDL.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_db::config::DbRole;
use afd_db::migration::Migration;
use afd_db::test_util::TestDatabase;
use afd_db::{Db, Migrator};

const VERSION: i64 = 900_003;

/// The versions sqlx recorded as applied.
const APPLIED: &str = "SELECT version FROM audit._sqlx_migrations WHERE success ORDER BY version";
const FAILING: &[Migration] = &[Migration::for_test(
    VERSION,
    "900003_batch_probe.sql",
    "CREATE TABLE public.afd_batch_probe (id INTEGER PRIMARY KEY);
     INSERT INTO public.afd_missing_batch_target (id) VALUES (7);",
)];
const RECOVERED: &[Migration] = &[Migration::for_test(
    VERSION,
    "900003_batch_probe.sql",
    "CREATE TABLE public.afd_batch_probe (id INTEGER PRIMARY KEY);
     INSERT INTO public.afd_batch_probe (id) VALUES (7);",
)];

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn a_runtime_batch_failure_rolls_back_and_a_corrected_retry_applies_once() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    assert_failed_batch_leaves_nothing(&db).await;

    let recovered = Migrator::new()
        .with_migrations(RECOVERED)
        .run(&db)
        .await
        .expect("the corrected batch can create the rolled-back table");
    assert_eq!(recovered.applied, [VERSION]);
    assert_eq!(recovered.skipped, [] as [i64; 0]);
    assert_recovered_state(&db).await;

    let repeated = Migrator::new()
        .with_migrations(RECOVERED)
        .run(&db)
        .await
        .expect("an applied migration is skipped, including its non-idempotent SQL");
    assert_eq!(repeated.applied, [] as [i64; 0]);
    assert_eq!(repeated.skipped, [VERSION]);
    assert_recovered_state(&db).await;

    db.close().await;
    database.cleanup().await;
}

async fn assert_failed_batch_leaves_nothing(db: &Db) {
    let error = Migrator::new()
        .with_migrations(FAILING)
        .run(db)
        .await
        .expect_err("the second statement references a missing relation");
    assert!(error.is_migration_failed(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.expect("inspect the failed migration");
    let applied: Vec<i64> = sqlx::query_scalar(APPLIED)
        .fetch_all(&mut *connection)
        .await
        .expect("read the ledger");
    assert!(
        applied.is_empty(),
        "a failed batch is never recorded: {applied:?}"
    );
    assert!(
        error.to_string().contains("900003"),
        "names the slot: {error}"
    );

    let created: bool =
        sqlx::query_scalar("SELECT to_regclass('public.afd_batch_probe') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await
            .expect("inspect the first statement's effect");
    assert!(
        !created,
        "the successful first statement must be rolled back"
    );
}

async fn assert_recovered_state(db: &Db) {
    let mut connection = db.acquire().await.expect("inspect the recovered migration");
    let applied: Vec<i64> = sqlx::query_scalar(APPLIED)
        .fetch_all(&mut *connection)
        .await
        .expect("read the ledger");
    assert_eq!(applied, [VERSION]);
    let rows: Vec<i32> = sqlx::query_scalar("SELECT id FROM public.afd_batch_probe ORDER BY id")
        .fetch_all(&mut *connection)
        .await
        .expect("read the committed batch effect");
    assert_eq!(rows, [7], "retry leaves exactly one committed batch effect");
}
