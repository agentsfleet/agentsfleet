//! Dimension 2.3 — a table whose rows change carries `updated_at`; an immutable
//! table lacks it, grants no runtime role UPDATE, and refuses UPDATE by trigger,
//! while a cascade from its parent still deletes it (rule 9).

use afd_db::test_util::mint_id;

use super::*;

/// The immutable tables: rows written once, then read or removed.
const IMMUTABLE: [&str; 11] = [
    "authoring.activity_receipts",
    "authoring.answer_receipts",
    "authoring.candidates",
    "authoring.library_revisions",
    "authoring.replay_receipts",
    "billing.authoring_report_receipts",
    "billing.authoring_settlements",
    "fleet.revisions",
    "identity.memberships",
    "identity.workspaces",
    "runner.events",
];

/// Every runtime role, none of which may update an immutable table.
const RUNTIME_ROLES: [&str; 4] = [
    "api_runtime",
    "memory_runtime",
    "ops_readonly_human",
    "ops_readonly_fleet",
];

/// Product tables that have no `updated_at` column.
const WITHOUT_UPDATED_AT: &str = "\
SELECT t.table_schema || '.' || t.table_name
  FROM information_schema.tables t
 WHERE t.table_type = 'BASE TABLE'
   AND t.table_schema NOT IN ('pg_catalog', 'information_schema', 'audit')
   AND NOT EXISTS (
       SELECT 1 FROM information_schema.columns c
        WHERE c.table_schema = t.table_schema AND c.table_name = t.table_name
          AND c.column_name = 'updated_at')
 ORDER BY 1";

/// Whether the table has a row-level BEFORE UPDATE trigger (`tgtype` bits:
/// 1 row, 2 before, 16 update).
const REFUSES_UPDATE: &str = "\
SELECT EXISTS (
    SELECT 1 FROM pg_trigger
     WHERE tgrelid = $1::regclass AND NOT tgisinternal AND (tgtype & 19) = 19)";

const UPDATE_GRANTED: &str = "SELECT has_any_column_privilege($1, $2, 'UPDATE')";

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_immutable_tables_refuse_update() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let without: Vec<String> = sqlx::query_scalar(WITHOUT_UPDATED_AT)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert_eq!(
        without, IMMUTABLE,
        "exactly the immutable tables lack updated_at"
    );

    for table in IMMUTABLE {
        let refuses: bool = sqlx::query_scalar(REFUSES_UPDATE)
            .bind(table)
            .fetch_one(&mut *connection)
            .await
            .unwrap();
        assert!(refuses, "{table} has no trigger refusing UPDATE");
        for role in RUNTIME_ROLES {
            let granted: bool = sqlx::query_scalar(UPDATE_GRANTED)
                .bind(role)
                .bind(table)
                .fetch_one(&mut *connection)
                .await
                .unwrap();
            assert!(!granted, "{role} may UPDATE immutable {table}");
        }
    }

    assert_refused_yet_cascaded(&mut connection).await;
    drop(connection);
    db.close().await;
    database.cleanup().await;
}

/// The trigger refuses even the owner's UPDATE, and deleting the parent still
/// removes the row: the trigger guards UPDATE only.
async fn assert_refused_yet_cascaded(connection: &mut PgConnection) {
    let tenant = mint_id();
    let workspace = mint_id();
    sqlx::query("INSERT INTO identity.tenants (id, name, created_at, updated_at) VALUES ($1::uuid, $2, 1, 1)")
        .bind(&tenant)
        .bind("tenant")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("INSERT INTO identity.workspaces (id, tenant_id, created_at) VALUES ($1::uuid, $2::uuid, 1)")
        .bind(&workspace)
        .bind(&tenant)
        .execute(&mut *connection)
        .await
        .unwrap();

    let update = sqlx::query("UPDATE identity.workspaces SET created_at = 2 WHERE id = $1::uuid")
        .bind(&workspace)
        .execute(&mut *connection)
        .await;
    assert!(update.is_err(), "an immutable row was updated");

    sqlx::query("DELETE FROM identity.tenants WHERE id = $1::uuid")
        .bind(&tenant)
        .execute(&mut *connection)
        .await
        .unwrap();
    let left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM identity.workspaces WHERE id = $1::uuid")
            .bind(&workspace)
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    assert_eq!(left, 0, "the cascade from the tenant removes the workspace");
}
