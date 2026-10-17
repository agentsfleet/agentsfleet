//! Migrating a live database: the ledger sqlx writes, and the schema it builds.
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
use afd_db::migrate::RetryPolicy;
use afd_db::migration::{MIGRATIONS, Migration};
use afd_db::test_util::TestDatabase;
use sqlx::PgConnection;
use sqlx::Row as _;

/// The versions the canonical list carries, as a set.
fn canonical_versions() -> BTreeSet<i64> {
    MIGRATIONS.iter().map(Migration::version).collect()
}

/// The versions sqlx recorded as applied.
async fn applied_versions(connection: &mut PgConnection) -> BTreeSet<i64> {
    sqlx::query_scalar("SELECT version FROM audit._sqlx_migrations WHERE success")
        .fetch_all(connection)
        .await
        .unwrap()
        .into_iter()
        .collect()
}

/// A fresh database, migrated by the canonical list.
async fn migrated() -> (TestDatabase, afd_db::Db) {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new()
        .run(&db)
        .await
        .expect("an empty database migrates");
    (database, db)
}

/// A fresh database ends up with exactly the canonical versions recorded, each
/// with its checksum, and a second run applies nothing.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_parity_fresh_db() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let outcome = Migrator::new().run(&db).await.expect("fresh migrate");
    let expected: Vec<i64> = MIGRATIONS.iter().map(Migration::version).collect();
    assert_eq!(
        outcome.applied, expected,
        "every canonical version, in order"
    );
    assert!(outcome.skipped.is_empty(), "nothing was applied before");

    let mut connection = db.acquire().await.unwrap();
    assert_eq!(
        applied_versions(&mut connection).await,
        canonical_versions()
    );
    let unsummed: i64 =
        sqlx::query("SELECT count(*) FROM audit._sqlx_migrations WHERE length(checksum) = 0")
            .fetch_one(&mut *connection)
            .await
            .unwrap()
            .get(0);
    assert_eq!(unsummed, 0, "every slot is recorded with its checksum");
    drop(connection);

    let second = Migrator::new().run(&db).await.expect("idempotent migrate");
    assert!(
        second.applied.is_empty(),
        "a slot applied twice is not versioned"
    );
    assert_eq!(second.skipped, expected);

    db.close().await;
    database.cleanup().await;
}

mod edited;
mod immutable;
mod instants;
mod literals;
mod locking;
mod names;
mod roles;
mod shape;
