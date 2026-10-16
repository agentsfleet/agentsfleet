//! Slot 950 against a populated database, and a release that fails inside it.
//!
//! Each proof migrates a scratch database to slot 949, writes rows into the
//! storage 950 drops and into the native setup that replaces it, then carries
//! it forward with the production migrator. The shared lane database is never
//! touched: a cutover needs a schema older than the one the lane holds.

use super::cutover_catalog::{dangling, storage};
use super::cutover_rows::{
    CUTOVER, Populated, at_slot_before_cutover, before_cutover, bundle_rows, setup_chain,
    survivors, through_cutover, versions_before_cutover,
};
use super::*;
use afd_db::Db;

/// The release-blocking dependency an operator might have left behind.
///
/// `DROP COLUMN` without `CASCADE` refuses while a view reads the column, and
/// 950 reaches its `ALTER TABLE` only after both `DROP TABLE`s have run, so
/// this fails the release partway through its one transaction.
const BLOCKER: &str =
    "CREATE VIEW public.cutover_blocker AS SELECT id, bundle_snapshot_key FROM core.fleets";
const UNBLOCK: &str = "DROP VIEW public.cutover_blocker";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_cutover_drops_bundle_storage_and_keeps_every_native_row() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let populated = at_slot_before_cutover(&db).await;
    let before = survivors(&db, &populated).await;
    assert_eq!(
        storage(&db).await,
        (2, 6),
        "949 holds two tables, six columns"
    );
    assert!(
        dangling(&db).await > 0,
        "the detector sees the storage's indexes"
    );

    let carried = Migrator::new()
        .with_migrations(through_cutover())
        .run(&db)
        .await
        .expect("950 applies to live rows");
    assert_eq!(carried.applied, [CUTOVER]);
    assert_eq!(carried.skipped, versions_before_cutover());
    assert_cut_over(&db, &populated, &before).await;

    let repeated = Migrator::new()
        .with_migrations(through_cutover())
        .run(&db)
        .await
        .expect("a rerun is idempotent");
    assert_eq!(repeated.applied, [] as [i32; 0]);
    assert_cut_over(&db, &populated, &before).await;

    db.close().await;
    database.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_failed_cutover_release_keeps_data_refuses_image_rollback_and_rolls_forward() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    let populated = at_slot_before_cutover(&db).await;
    let before = survivors(&db, &populated).await;
    let stored = bundle_rows(&db, &populated).await;
    let own = dangling(&db).await;
    execute(&db, BLOCKER).await;
    assert_eq!(dangling(&db).await, own + 1, "the detector sees the view");

    let error = Migrator::new()
        .with_migrations(through_cutover())
        .run(&db)
        .await
        .expect_err("a view on a dropped column fails the release");
    assert!(error.is_migration_failed(), "got {error}");
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");
    assert!(error.to_string().contains("950"), "names the slot: {error}");
    assert_recoverable(&db, &populated, &before, &stored).await;

    let rollback = Migrator::new()
        .with_migrations(before_cutover())
        .run(&db)
        .await
        .expect_err("the previous image's migrate meets a newer failure row");
    assert!(rollback.is_migration_refused(), "got {rollback}");
    assert_recoverable(&db, &populated, &before, &stored).await;

    execute(&db, UNBLOCK).await;
    let forward = Migrator::new()
        .with_migrations(through_cutover())
        .run(&db)
        .await
        .expect("roll-forward applies 950");
    assert_eq!(forward.applied, [CUTOVER]);
    assert_eq!(forward.skipped, versions_before_cutover());
    assert_cut_over(&db, &populated, &before).await;

    db.close().await;
    database.cleanup().await;
}

/// The failed release left exactly the previous release's schema, data and ledger.
async fn assert_recoverable(db: &Db, populated: &Populated, before: &str, stored: &str) {
    let ledger = ledger(db).await;
    assert_eq!(
        ledger.applied,
        versions_before_cutover()
            .into_iter()
            .collect::<BTreeSet<_>>(),
        "nothing from the failed slot is recorded applied"
    );
    let failure = ledger
        .failures
        .get(&CUTOVER)
        .expect("the failure is recorded");
    assert!(
        failure.error_text.contains("depend"),
        "the operator is told what blocked it: {}",
        failure.error_text
    );
    assert_eq!(ledger.failures.len(), 1);
    assert_eq!(
        storage(db).await,
        (2, 6),
        "the DROP TABLEs that ran before the failure rolled back with it"
    );
    assert_eq!(&survivors(db, populated).await, before);
    assert_eq!(
        &bundle_rows(db, populated).await,
        stored,
        "bundle data intact"
    );
}

async fn assert_cut_over(db: &Db, populated: &Populated, before: &str) {
    assert_eq!(storage(db).await, (0, 0), "no table or column survives 950");
    assert_eq!(dangling(db).await, 0, "no catalog object names the storage");
    assert_eq!(
        &survivors(db, populated).await,
        before,
        "every surviving row keeps every surviving column"
    );
    assert_eq!(
        setup_chain(db, populated).await,
        1,
        "the fleet's setup resolves through native revisions"
    );
    // Through the cutover, not the whole list: these proofs stop at 950, and
    // the slots after it carry their own populated proof in `retirement`.
    let ledger = ledger(db).await;
    assert_eq!(
        ledger.applied,
        through_cutover()
            .iter()
            .map(Migration::version)
            .collect::<BTreeSet<_>>()
    );
    assert!(
        ledger.failures.is_empty(),
        "a recovered slot clears its failure"
    );
}

async fn ledger(db: &Db) -> Ledger {
    Ledger::read(&mut db.acquire().await.unwrap())
        .await
        .unwrap()
}

async fn execute(db: &Db, statement: &'static str) {
    sqlx::query(statement)
        .execute(&mut *db.acquire().await.unwrap())
        .await
        .unwrap();
}
