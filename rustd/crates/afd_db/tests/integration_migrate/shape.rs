//! Dimension 2.1 — migrating an empty database builds the nine owned schemas,
//! each holding a table, and no table anywhere else.
//!
//! `schema/*.sql` is the only record of each table's columns, so nothing here
//! copies them: the test reads the migrated database and checks only where its
//! tables live.

use super::*;

/// The nine owned schemas. `audit` holds only sqlx's ledger.
const OWNED_SCHEMAS: [&str; 9] = [
    "identity",
    "authoring",
    "fleet",
    "runner",
    "connector",
    "model",
    "billing",
    "vault",
    "memory",
];

/// The schema the migrator creates for sqlx's ledger.
const LEDGER_SCHEMA: &str = "audit";
/// sqlx's ledger: the migrator's, not a slot's.
const LEDGER_TABLE: &str = "audit._sqlx_migrations";

/// Every table outside the system schemas.
const BUILT_TABLES: &str = "\
SELECT table_schema || '.' || table_name
  FROM information_schema.tables
 WHERE table_type = 'BASE TABLE'
   AND table_schema NOT IN ('pg_catalog', 'information_schema')";

/// Every schema the database holds beyond the system ones and `public`.
const BUILT_SCHEMAS: &str = "\
SELECT nspname::text FROM pg_namespace
 WHERE nspname NOT LIKE 'pg\\_%' AND nspname NOT IN ('information_schema', 'public')";

/// The tables the migrated database holds, ledger excluded.
async fn built(connection: &mut PgConnection) -> BTreeSet<String> {
    let tables: Vec<String> = sqlx::query_scalar(BUILT_TABLES)
        .fetch_all(connection)
        .await
        .unwrap();
    tables
        .into_iter()
        .filter(|table| table != LEDGER_TABLE)
        .collect()
}

/// The schema half of a `schema.table` name.
fn schema_of(table: &str) -> &str {
    table.split('.').next().unwrap_or_default()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_migrate_from_empty_builds_nine_owned_schemas() {
    let owned: BTreeSet<&str> = OWNED_SCHEMAS.into_iter().collect();

    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();
    let built = built(&mut connection).await;
    let schemas: Vec<String> = sqlx::query_scalar(BUILT_SCHEMAS)
        .fetch_all(&mut *connection)
        .await
        .unwrap();

    let mut expected_schemas = owned.clone();
    expected_schemas.insert(LEDGER_SCHEMA);
    let built_schemas: BTreeSet<&str> = schemas.iter().map(String::as_str).collect();
    assert_eq!(
        built_schemas, expected_schemas,
        "the schemas the slots create"
    );
    let table_schemas: BTreeSet<&str> = built.iter().map(|t| schema_of(t)).collect();
    assert_eq!(
        table_schemas, owned,
        "every owned schema holds a table, and only they do"
    );

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
