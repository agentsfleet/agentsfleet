//! Reserved-gap migration ledger safety.

use super::*;

/// A migration a NEWER release put in a reserved gap is refused and kept.
///
/// The numbering leaves gaps on purpose — `550`, `551`, `560` — so a release
/// that fills one lands BELOW an older binary's ceiling, and a check against
/// the ceiling alone would miss it. Both halves are asserted, because the
/// refusal alone is not the claim:
///
///   1. the run is refused, naming the gap version it did not know, and
///   2. the gap row is STILL THERE afterwards.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_a_gap_migration_from_a_newer_release_is_refused_and_left_alone() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&db).await.expect("first migrate");

    let migrator = Migrator::new();
    let canonical = migrator.canonical_versions();
    let floor = canonical
        .iter()
        .copied()
        .min()
        .expect("the canonical list is not empty");
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
        "the newer release's record must survive a refused run"
    );
    drop(connection);

    db.close().await;
    database.cleanup().await;
}
