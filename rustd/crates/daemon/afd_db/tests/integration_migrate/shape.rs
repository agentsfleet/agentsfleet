//! Migrating an empty database builds the nine owned schemas,
//! each holding a table, and no table anywhere else.
//!
//! `schema/*.sql` is the only record of each table's columns, so nothing here
//! copies them: the test reads the migrated database and checks only where its
//! tables live.

use super::*;

/// The nine owned schemas. `audit` holds only sqlx's ledger.
const OWNED_SCHEMAS: [&str; 9] = [
    "identity",
    "fleet",
    "runner",
    "thread",
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

/// The columns of the schemas in `$1` and the tables in `$2` that name a draft
/// or a revision.
const DRAFT_OR_REVISION_COLUMNS: &str = "\
SELECT table_schema || '.' || table_name || '.' || column_name
  FROM information_schema.columns
 WHERE (table_schema = ANY($1) OR table_schema || '.' || table_name = ANY($2))
   AND (column_name = 'draft_id' OR column_name LIKE '%revision\\_id')";

/// The columns [`DRAFT_OR_REVISION_COLUMNS`] finds in `schemas` and `tables`.
async fn draft_or_revision_columns(
    connection: &mut PgConnection,
    schemas: &[&str],
    tables: &[&str],
) -> Vec<String> {
    sqlx::query_scalar(DRAFT_OR_REVISION_COLUMNS)
        .bind(schemas)
        .bind(tables)
        .fetch_all(connection)
        .await
        .unwrap()
}

/// Whether `thread.threads.fleet_id` admits NULL.
const THREAD_FLEET_NULLABLE: &str = "\
SELECT is_nullable FROM information_schema.columns
 WHERE table_schema = 'thread' AND table_name = 'threads' AND column_name = 'fleet_id'";

/// Every CHECK `thread.threads` carries, as Postgres renders it.
const THREAD_CHECKS: &str = "\
SELECT pg_get_constraintdef(con.oid)
  FROM pg_constraint con
 WHERE con.conrelid = 'thread.threads'::regclass AND con.contype = 'c'";

/// No draft or revision survives migration: no `setup` schema, no
/// `fleet.revisions`, no column naming either, and a thread belongs to exactly
/// the fleet its id names.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn the_schema_names_no_draft_or_revision() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let schemas: Vec<String> = sqlx::query_scalar(BUILT_SCHEMAS)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert!(
        !schemas.iter().any(|schema| schema == "setup"),
        "{schemas:?}"
    );
    let built = built(&mut connection).await;
    assert!(
        built.contains("fleet.fleets"),
        "the catalog read finds a known table"
    );
    assert!(!built.contains("fleet.revisions"), "{built:?}");
    assert!(!built.contains("setup.drafts"), "{built:?}");

    // The same read finds a revision column where one belongs: the catalog's.
    let catalogue = draft_or_revision_columns(&mut connection, &["model"], &[]).await;
    assert!(
        catalogue.contains(&"model.platform_models.decision_revision_id".to_owned()),
        "{catalogue:?}"
    );
    let named = draft_or_revision_columns(
        &mut connection,
        &["fleet", "thread"],
        &["runner.checkpoints"],
    )
    .await;
    assert_eq!(named, Vec::<String>::new());

    let nullable: String = sqlx::query_scalar(THREAD_FLEET_NULLABLE)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(nullable, "NO");
    let checks: Vec<String> = sqlx::query_scalar(THREAD_CHECKS)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert!(
        checks.iter().any(|check| check.contains("id = fleet_id")),
        "{checks:?}"
    );

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
