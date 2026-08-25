//! The migrator's failure bookkeeping, against a live database.
//!
//! `integration_migrate.rs` proves what a migrate does when it works. These
//! prove what it does when it does not, and the distinction they all turn on is
//! the same one: **a bookkeeping write that fails must never replace the cause
//! it was trying to record.** A migration that fails and a ledger that cannot
//! say so are two problems, and an operator who is shown only the second one
//! goes looking in the wrong place.
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
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_db::Migrator;
use afd_db::config::DbRole;
use afd_db::migrate::{MigrationLock, RetryPolicy, ledger};
use afd_db::migration::Migration;
use sqlx::Row as _;

#[path = "support/test_database.rs"]
mod support;

use self::support::TestDatabase;

/// A migration whose SQL ends inside a string literal.
///
/// The split has to refuse this rather than hand Postgres the half of it that
/// parses: `'` opens a literal that never closes, so a splitter that broke on
/// the next `;` would submit a fragment.
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
    "DROP TABLE audit.schema_migrations",
)];

/// A retry policy that gives up quickly; nothing here contends for the lock.
const IMPATIENT: RetryPolicy = RetryPolicy::new(3, Duration::from_millis(20));

/// A migrator that refuses a newer schema accepts one that is merely current.
///
/// The refusing policy has two outcomes and the interesting one is usually the
/// refusal — but a policy that refused a database it had nothing to complain
/// about would stop every deployment that runs it, which is the failure nobody
/// would catch in review because the happy path is the one nobody writes down.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_refusing_migrator_accepts_a_ledger_that_is_not_ahead() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    Migrator::new().run(&db).await.expect("the first migrate");

    let outcome = Migrator::new()
        .refusing_newer()
        .run(&db)
        .await
        .expect("a ledger at exactly the canonical set must not be refused");
    assert!(
        outcome.applied.is_empty(),
        "the second run has nothing left to apply"
    );

    db.close().await;
    database.cleanup().await;
}

/// A migration whose SQL cannot be split is refused before any of it runs, and
/// the refusal is recorded.
///
/// Refused BEFORE, which is the whole claim: the alternative is submitting the
/// prefix that happens to parse and leaving the schema half-changed, with a
/// ledger that says the migration never ran. The failure row is what turns a
/// broken deploy into something the next boot can report.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_migration_that_cannot_be_split_is_refused_and_recorded() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let error = Migrator::new()
        .with_migrations(UNTERMINATED)
        .run(&db)
        .await
        .expect_err("a migration that ends inside a literal must be refused");

    assert!(error.is_migration_failed(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.unwrap();
    let recorded: i64 =
        sqlx::query("SELECT count(*) FROM audit.schema_migration_failures WHERE version = 9001")
            .fetch_one(&mut *connection)
            .await
            .unwrap()
            .get(0);
    assert_eq!(recorded, 1, "the refusal must leave a failure row behind");

    // Nothing applied: the statement before the broken one must not have run.
    let created: bool = sqlx::query("SELECT to_regclass('public.never_created') IS NOT NULL")
        .fetch_one(&mut *connection)
        .await
        .unwrap()
        .get(0);
    assert!(
        !created,
        "the split is refused whole; no statement in the file may apply"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A migration that invalidates the ledger mid-transaction fails with the
/// bookkeeping error, and applies nothing.
///
/// The transaction is what makes this safe: the schema change and the row
/// recording it commit together, so an insert that cannot run rolls the DDL
/// back with it. What the caller must get is the query error naming the insert
/// — not a success, and not the migration's own error, because the migration
/// did not fail.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_migration_that_breaks_its_own_bookkeeping_rolls_back() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    let error = Migrator::new()
        .with_migrations(DROPS_THE_LEDGER)
        .run(&db)
        .await
        .expect_err("an insert into a dropped table must fail the migration");

    assert!(
        error.is_query(),
        "the bookkeeping insert failed, not the migration: {error}"
    );

    // The rollback restored the table the migration dropped.
    let mut connection = db.acquire().await.unwrap();
    let ledger_exists: bool =
        sqlx::query("SELECT to_regclass('audit.schema_migrations') IS NOT NULL")
            .fetch_one(&mut *connection)
            .await
            .unwrap()
            .get(0);
    assert!(
        ledger_exists,
        "the transaction rolled back, so the ledger table must still be there"
    );

    // And the failure was still recorded — on the connection, outside the
    // transaction that rolled back, which is why it survives.
    let recorded: i64 =
        sqlx::query("SELECT count(*) FROM audit.schema_migration_failures WHERE version = 9002")
            .fetch_one(&mut *connection)
            .await
            .unwrap()
            .get(0);
    assert_eq!(
        recorded, 1,
        "the failure row outlives the rolled-back apply"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// Bookkeeping writes that cannot run are logged and swallowed, never raised.
///
/// Both of these are called on paths where something has ALREADY gone wrong, or
/// where the caller has no error to return at all. A `record_failure` that
/// propagated would replace the migration failure it was recording; a
/// `clear_failure` that propagated would fail a migrate that had succeeded.
/// Neither returns a `Result`, and this is what holds that they cannot start.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_bookkeeping_writes_that_cannot_run_are_swallowed() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    Migrator::new()
        .run(&db)
        .await
        .expect("the ledger tables have to exist before they can be removed");

    let mut connection = db.acquire().await.unwrap();
    sqlx::query("DROP TABLE audit.schema_migration_failures")
        .execute(&mut *connection)
        .await
        .unwrap();

    // Neither of these returns anything. Reaching the line after each one IS
    // the assertion: a propagated error would be a panic here.
    ledger::record_failure(&mut connection, 1, "a failure nothing can record").await;
    ledger::clear_failure(&mut connection, 1).await;

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

/// An empty migration list reaps nothing, rather than emptying the ledger.
///
/// The degenerate input, asserted because the safe answer is not the obvious
/// one. `reap_orphans` deletes everything below the lowest version it knows,
/// and the lowest version of an empty list is not a number — so the reflex is
/// to treat "no floor" as "no bound" and delete the lot. That reflex would let
/// a binary built with no migrations wipe the migration history of a database
/// it knows nothing about, which is the same corruption the floor exists to
/// prevent, arrived at from the other side.
///
/// An empty list is a `Migrator` that has nothing to say about this database.
/// Saying nothing is the correct thing to do to it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_an_empty_migration_list_reaps_nothing() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let applied_before = {
        let mut connection = db.acquire().await.unwrap();
        ledger::Ledger::read(&mut connection).await.unwrap().applied
    };
    assert!(
        !applied_before.is_empty(),
        "the precondition is a ledger with something in it to destroy"
    );

    let mut connection = db.acquire().await.unwrap();
    let reaped = ledger::reap_orphans(&mut connection, &[])
        .await
        .expect("reaping against an empty canonical list is not an error");
    assert_eq!(
        reaped, 0,
        "no floor means no deletion, not unbounded deletion"
    );

    let applied_after = ledger::Ledger::read(&mut connection).await.unwrap().applied;
    assert_eq!(
        applied_after, applied_before,
        "every recorded version survives a migrator that knows none"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}

/// A migration a NEWER release put in a reserved gap is refused, not reaped.
///
/// The numbering leaves gaps on purpose — `550`, `551`, `560` — so a release
/// that fills one lands BELOW an older binary's ceiling. A ceiling test reads
/// that as a retired slot and deletes its record while the schema change it
/// records stays applied: the history is corrupted, and the older binary then
/// applies its own migrations to a schema it does not understand. The
/// discriminator is the FLOOR, not the ceiling, and this is the case that
/// tells the two apart.
///
/// Both halves are asserted, because the refusal alone is not the claim. A run
/// that refused AFTER reaping would pass an exit-code check and still have
/// destroyed the record:
///
///   1. the run is refused, naming the gap version it did not know, and
///   2. the gap row is STILL THERE afterwards.
///
/// `Migrator::new()` is deliberate — the production configuration, not
/// `refusing_newer()`. A gap migration is below the ceiling, so the policy that
/// used to guard this was the one production does not run.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_gap_migration_from_a_newer_release_is_refused_and_left_alone() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let migrator = Migrator::new();
    let canonical = migrator.canonical_versions();
    let floor = migrator.floor().expect("the canonical list is not empty");
    let ceiling = migrator.ceiling().expect("the canonical list is not empty");
    // Inside the range this scheme owns, below the ceiling, absent from the
    // list — exactly the shape a newer release adds.
    let gap = (floor..ceiling)
        .find(|version| !canonical.contains(version))
        .expect("the numbering reserves gaps between slots");

    let mut connection = db.acquire().await.unwrap();
    sqlx::query("INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, 1)")
        .bind(gap)
        .execute(&mut *connection)
        .await
        .unwrap();
    drop(connection);

    let error = Migrator::new()
        .run(&db)
        .await
        .expect_err("a gap migration from a newer release must refuse the run");
    assert!(error.is_migration_refused(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");
    assert!(
        error.to_string().contains(&gap.to_string()),
        "the refusal names the gap version it did not know: {error}"
    );

    let mut connection = db.acquire().await.unwrap();
    let recorded = ledger::Ledger::read(&mut connection).await.unwrap();
    assert!(
        recorded.applied.contains(&gap),
        "the newer release's record must survive a refused run — reaping it is the corruption"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}
