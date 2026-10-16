//! Dimensions 2.1–2.3 — migrating a live database, and the ledger it writes.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migrate::{Ledger, RetryPolicy};
use afd_db::migration::{MIGRATIONS, Migration};
use afd_db::test_util::TestDatabase;
use sqlx::Row as _;

/// The versions the canonical list carries, as a set.
fn canonical_versions() -> BTreeSet<i32> {
    MIGRATIONS.iter().map(Migration::version).collect()
}

/// Dimension 2.1 — a fresh database ends up with exactly the canonical
/// versions recorded, and a second run applies nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_parity_fresh_db() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let outcome = Migrator::new().run(&db).await.expect("fresh migrate");
    let expected: Vec<i32> = MIGRATIONS.iter().map(Migration::version).collect();
    assert_eq!(
        outcome.applied, expected,
        "a fresh database must apply every canonical version, in order"
    );
    assert!(outcome.skipped.is_empty(), "nothing was applied before");

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert_eq!(
        ledger.applied,
        canonical_versions(),
        "audit.schema_migrations must hold exactly the canonical versions"
    );
    assert!(
        ledger.failures.is_empty(),
        "a clean run leaves no failure rows: {:?}",
        ledger.failures
    );

    // The bookkeeping row carries a real timestamp, not a zero — `applied_at`
    // is what an operator reads to tell one deploy's migrations from another's.
    let earliest: i64 = sqlx::query("SELECT MIN(applied_at) FROM audit.schema_migrations")
        .fetch_one(&mut *connection)
        .await
        .unwrap()
        .try_get(0)
        .unwrap();
    assert!(earliest > 1_700_000_000_000, "applied_at was {earliest}");
    drop(connection);

    // Once each: the second run recognises everything and applies nothing.
    let second = Migrator::new().run(&db).await.expect("idempotent migrate");
    assert!(
        second.applied.is_empty(),
        "a migration applied twice is a migration that is not versioned"
    );
    assert_eq!(second.skipped, expected);

    db.close().await;
    database.cleanup().await;
}

/// A version below every canonical slot refuses the run and is left alone.
///
/// No version is retired by deleting its row: whatever wrote one this binary
/// does not know, the ledger keeps it and the run changes nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_version_below_the_canonical_list_refuses_the_run() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let mut connection = db.acquire().await.unwrap();
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES (42, 1)")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let error = Migrator::new()
        .run(&db)
        .await
        .expect_err("a version this binary does not know must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert!(ledger.applied.contains(&42), "the unknown row survives");
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

mod compatibility;
mod cutover;
mod cutover_catalog;
mod cutover_rows;
mod locking;
mod retirement;
mod roles;
mod runner_policy;
