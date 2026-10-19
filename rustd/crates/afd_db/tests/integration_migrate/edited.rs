//! Dimension 2.5 — an applied slot whose file changed is refused by checksum,
//! naming its version, and the run applies nothing after it.

use std::sync::LazyLock;

use super::*;

/// The slot as it was first applied.
static ORIGINAL: LazyLock<[Migration; 1]> = LazyLock::new(|| {
    [Migration::for_test(
        900_101,
        "900101_creates_a_table.sql",
        "CREATE TABLE afd_edited_probe (id INTEGER PRIMARY KEY);",
    )]
});

/// The same version with its file edited after release, plus a later slot that
/// must not apply behind the refusal.
static EDITED: LazyLock<[Migration; 2]> = LazyLock::new(|| {
    [
        Migration::for_test(
            900_101,
            "900101_creates_a_table.sql",
            "CREATE TABLE afd_edited_probe (id BIGINT PRIMARY KEY);",
        ),
        Migration::for_test(
            900_102,
            "900102_after_the_edit.sql",
            "CREATE TABLE afd_after_edit_probe (id INTEGER PRIMARY KEY);",
        ),
    ]
});

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_refuses_edited_slot() {
    let database = TestDatabase::create().await;
    let db = database.open(DbRole::Migrator, &[]).await;

    Migrator::new()
        .with_migrations(&*ORIGINAL)
        .run(&db)
        .await
        .expect("the original slot applies");

    let error = Migrator::new()
        .with_migrations(&*EDITED)
        .run(&db)
        .await
        .expect_err("an applied slot whose file changed must be refused");
    assert!(error.is_migration_edited(), "got {error}");
    assert!(
        error.is_migration_refused(),
        "an edit is a refusal: {error}"
    );
    assert!(
        error.to_string().contains("900101"),
        "names the slot: {error}"
    );
    assert_eq!(error.code().as_str(), "UZ-STARTUP-005");

    let mut connection = db.acquire().await.unwrap();
    let applied = applied_versions(&mut connection).await;
    assert!(
        !applied.contains(&900_102),
        "nothing applies behind the refusal"
    );
    let later: bool = sqlx::query("SELECT to_regclass('public.afd_after_edit_probe') IS NOT NULL")
        .fetch_one(&mut *connection)
        .await
        .unwrap()
        .get(0);
    assert!(!later, "the later slot's table was never created");
    drop(connection);

    db.close().await;
    database.cleanup().await;
}
