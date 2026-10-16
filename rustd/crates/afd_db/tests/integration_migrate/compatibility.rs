//! Compatibility refusals for migration ledgers written by newer binaries.

use super::*;

/// An older binary meeting a database a newer one migrated changes nothing.
///
/// An older image rolled onto an upgraded database must not apply its own
/// schema on top of one it does not understand. Both halves are asserted,
/// because the refusal alone is not the claim:
///
///   1. the run is refused, naming the version it did not know, and
///   2. the newer row is STILL THERE afterwards.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_ledger_from_a_newer_binary_is_refused_and_left_alone() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    // Above every canonical version: a slot this binary has never heard of.
    let newer = canonical_versions()
        .into_iter()
        .max()
        .expect("the canonical list is not empty")
        + 10;

    let mut connection = db.acquire().await.unwrap();
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, 1)")
        .bind(newer)
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let error = Migrator::new()
        .run(&db)
        .await
        .expect_err("a database migrated by a newer binary must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");
    assert!(
        error.to_string().contains(&newer.to_string()),
        "the refusal names the version it did not know: {error}"
    );

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert!(
        ledger.applied.contains(&newer),
        "the newer binary's history must survive a refused run"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A failure row from a newer binary refuses the run just as an applied row does.
///
/// Both bookkeeping tables are evidence of the same thing: something newer has
/// been here. Checking only `schema_migrations` would let an older image
/// overwrite a database whose newer migration FAILED — the worst moment to do
/// it, because that database is already half-migrated.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_newer_failure_row_also_refuses_the_run() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let newer = canonical_versions()
        .into_iter()
        .max()
        .expect("the canonical list is not empty")
        + 20;

    let mut connection = db.acquire().await.unwrap();
    sqlx::query(
        "INSERT INTO audit.schema_migration_failures (version, failed_at, error_text) \
         VALUES ($1, 1, 'a newer binary tried and failed')",
    )
    .bind(newer)
    .execute(&mut *connection)
    .await
    .unwrap();
    drop(connection);

    let error = Migrator::new()
        .run(&db)
        .await
        .expect_err("a failure row from a newer binary must refuse the run too");
    assert!(error.is_migration_refused(), "got {error}");

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert!(
        ledger.failures.contains_key(&newer),
        "the newer binary's failure row must survive a refused run"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}
