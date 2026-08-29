//! Empty-list and reserved-gap migration ledger safety.

use super::*;

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
