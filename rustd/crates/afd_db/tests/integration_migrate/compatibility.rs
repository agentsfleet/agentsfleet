//! Compatibility refusals for migration ledgers written by newer binaries.

use super::*;

/// An older binary meeting a database a newer one migrated changes nothing.
///
/// The failure this exists to prevent, in the order it would have happened:
/// `reap_orphans` deleted every version absent from the canonical list, and a
/// version written by a NEWER binary is absent from an older binary's list in
/// exactly the way a retired slot is. So an older image rolled onto an upgraded
/// database would delete the newer deployment's migration history, then apply
/// its own schema on top of one it does not understand — leaving the schema
/// split between two versions with no ledger left to say so.
///
/// Both halves are asserted, because the refusal alone is not the claim. A run
/// that refused AFTER reaping would pass an exit-code check and still have
/// destroyed the history:
///
///   1. the run is refused, naming the version it did not know, and
///   2. the newer row is STILL THERE afterwards.
///
/// `Migrator::new()` is deliberate — the production configuration, not
/// `refusing_newer()`. The whole finding was that the safe policy existed and
/// production did not use it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_ledger_from_a_newer_binary_is_refused_and_left_alone() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    // Above every canonical version: a slot this binary has never heard of and
    // could not have retired, which is what makes it a newer schema rather than
    // an older one's leftovers.
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
        "the newer binary's history must survive a refused run — reaping it is the corruption"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A failure row from a newer binary refuses the run just as an applied row does.
///
/// Both bookkeeping tables are evidence of the same thing: something newer has
/// been here. Checking only `schema_migrations` would let a database whose
/// newer migration FAILED be reaped and overwritten by an older image — the
/// worst moment to do it, because that database is already half-migrated.
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
