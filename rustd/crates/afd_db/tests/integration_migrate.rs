//! Dimensions 2.1–2.3 — migrating a live database, and the ledger it writes.
//!
//! Marked `#[ignore]` so `make test-unit-rustd` compiles and lints these
//! without needing a datastore; `make test-integration-rustd` runs them.
#![cfg(feature = "test-util")]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::collections::BTreeSet;
use std::time::Duration;

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migrate::{Ledger, RetryPolicy};
use afd_db::migration::{MIGRATIONS, Migration};
use sqlx::Row as _;

#[path = "support/test_database.rs"]
mod support;

use self::support::TestDatabase;

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

/// Dimension 2.1 (reap half) — a bookkeeping row whose version left the
/// canonical list is deleted, and the run is otherwise unaffected.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_reaps_orphaned_bookkeeping_rows() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let mut connection = db.acquire().await.unwrap();
    // A version from a binary that no longer exists — what the pre-v2.0
    // teardown left behind.
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES (42, 1)")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let outcome = Migrator::new().run(&db).await.expect("reaping migrate");
    assert_eq!(outcome.reaped, 1, "the orphan row must be deleted");

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert_eq!(ledger.applied, canonical_versions());
    drop(connection);

    // Refusing instead of reaping is the other policy, and it must refuse.
    let mut connection = db.acquire().await.unwrap();
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES (43, 1)")
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let error = Migrator::new()
        .refusing_newer()
        .run(&db)
        .await
        .expect_err("a version this binary does not know must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    db.close().await;
    database.cleanup().await;
}

/// Dimension 2.2 — two migrators on one fresh database: one applies, the other
/// waits and finds nothing to do. Never both.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_advisory_lock_contention() {
    let database = TestDatabase::create().await;
    let first = database.open(DbRole::Migrator, &[]).await;
    let second = database.open(DbRole::Migrator, &[]).await;

    // A bound long enough for the loser to outlast a full 47-migration apply,
    // short enough that a genuine deadlock fails the test rather than the lane.
    let policy = RetryPolicy::new(120, Duration::from_millis(250));
    let migrator = Migrator::new().with_retry_policy(policy);

    let (left, right) = tokio::join!(migrator.run(&first), migrator.run(&second));
    let left = left.expect("first migrator");
    let right = right.expect("second migrator");

    let applied_total = left.applied.len() + right.applied.len();
    assert_eq!(
        applied_total,
        MIGRATIONS.len(),
        "every version applies exactly once across both migrators"
    );
    assert!(
        left.applied.is_empty() || right.applied.is_empty(),
        "the lock must serialise them: one applies, the other no-ops"
    );

    let mut connection = first.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();
    assert_eq!(
        ledger.applied,
        canonical_versions(),
        "the ledger must hold one row per version, no duplicates"
    );
    drop(connection);

    first.close().await;
    second.close().await;
    database.cleanup().await;
}

/// Dimension 2.3 — a migration that fails records a failure row and is never
/// recorded as applied.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_failure_bookkeeping() {
    static FAILING: &[Migration] = &[
        Migration::for_test(
            900_001,
            "900001_creates_a_table.sql",
            "CREATE TABLE afd_migrate_probe (id INTEGER PRIMARY KEY);",
        ),
        Migration::for_test(
            900_002,
            "900002_refers_to_nothing.sql",
            "INSERT INTO a_table_that_does_not_exist (id) VALUES (1);",
        ),
    ];

    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let error = Migrator::new()
        .with_migrations(FAILING)
        .run(&db)
        .await
        .expect_err("a migration against a missing table must fail");
    assert!(error.is_migration_failed(), "got {error}");
    assert!(
        error.to_string().contains("900002"),
        "the failure must name the version: {error}"
    );

    let mut connection = db.acquire().await.unwrap();
    let ledger = Ledger::read(&mut connection).await.unwrap();

    assert!(
        ledger.applied.contains(&900_001),
        "the migration that succeeded stays applied"
    );
    assert!(
        !ledger.applied.contains(&900_002),
        "a failed migration must never be recorded as applied"
    );

    let failure = ledger
        .failures
        .get(&900_002)
        .expect("the failure must be recorded for an operator to find");
    assert!(
        !failure.error_text.is_empty(),
        "the failure row must say what went wrong"
    );
    assert!(failure.failed_at > 0, "failed_at must be a real timestamp");

    // The transaction rolled back, so the failing migration left nothing behind.
    let leftover: i64 = sqlx::query(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = 'afd_migrate_probe'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap()
    .try_get(0)
    .unwrap();
    assert_eq!(leftover, 1, "the successful migration's table must survive");
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A migrator that cannot get the lock gives up, loudly, inside its bound.
///
/// This is the stop path the whole bounded poll exists for: a crashed migrator
/// that never released the lock must not hang the next deploy until the deploy
/// machine's own timeout fires, minutes later, with nothing saying why.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_gives_up_when_the_lock_never_frees() {
    let database = TestDatabase::create().await;
    let holder = database.open(DbRole::Migrator, &[]).await;
    let waiter = database.open(DbRole::Migrator, &[]).await;

    // Take the advisory lock on a connection that simply keeps it.
    let mut held = holder.acquire().await.unwrap();
    let taken: bool = sqlx::query("SELECT pg_try_advisory_lock($1)")
        .bind(0x7A6F_6D62_6965_0001_i64)
        .fetch_one(&mut *held)
        .await
        .unwrap()
        .try_get(0)
        .unwrap();
    assert!(taken, "the test must actually hold the lock");

    let started = std::time::Instant::now();
    let error = Migrator::new()
        .with_retry_policy(RetryPolicy::new(3, Duration::from_millis(50)))
        .run(&waiter)
        .await
        .expect_err("a lock nobody releases must end the run, not extend it");

    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");
    assert!(
        error.to_string().contains("150"),
        "the failure must report how long it waited: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the bound is the stop path; this waited {:?}",
        started.elapsed()
    );

    drop(held);
    holder.close().await;
    waiter.close().await;
    database.cleanup().await;
}
