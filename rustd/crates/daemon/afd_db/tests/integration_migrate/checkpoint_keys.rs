//! No checkpoint table stores a key: the workspace's one checkpoint key is a
//! `vault.secrets` row, and `runner` holds only what that key sealed.

use super::*;

/// The `runner` columns whose names say they hold key material: an envelope's
/// wrapped key and payload, its wrapping version, or an identity key.
const KEY_COLUMNS: &str = "\
SELECT table_name || '.' || column_name
  FROM information_schema.columns
 WHERE table_schema = 'runner'
   AND (column_name LIKE 'dek\\_%' OR column_name LIKE '%\\_dek\\_%'
        OR column_name LIKE 'secret\\_%' OR column_name LIKE '%\\_secret\\_%'
        OR column_name LIKE '%kek\\_version' OR column_name LIKE '%identity\\_key%')
 ORDER BY 1";

/// The `runner` tables named for an identity key: the per-repository key table
/// the vault row replaced.
const IDENTITY_TABLES: &str = "\
SELECT count(*) FROM information_schema.tables
 WHERE table_schema = 'runner' AND table_name LIKE '%identity\\_key%'";

/// The checkpoint tables the search must have looked through to mean anything.
const SEARCHED: &str = "\
SELECT count(*) FROM information_schema.columns
 WHERE table_schema = 'runner' AND table_name IN ('checkpoints', 'checkpoint_objects')
   AND column_name IN ('root_sealed', 'object_nonce')";

async fn key_columns(connection: &mut PgConnection) -> Vec<String> {
    sqlx::query_scalar(KEY_COLUMNS)
        .fetch_all(connection)
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn no_checkpoint_table_holds_key_material() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let identity_tables: i64 = sqlx::query_scalar(IDENTITY_TABLES)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(identity_tables, 0, "the identity-key table is gone");
    assert_eq!(key_columns(&mut connection).await, Vec::<String>::new());
    let searched: i64 = sqlx::query_scalar(SEARCHED)
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    assert_eq!(searched, 2, "the search read the checkpoint tables");

    // A schema keeping one envelope column, in either spelling, is found.
    for kept in [
        "dek_nonce BYTEA",
        "root_secret_tag BYTEA",
        "kek_version INTEGER",
    ] {
        let mut tx = sqlx::Connection::begin(&mut *connection).await.unwrap();
        let statement = format!("ALTER TABLE runner.checkpoints ADD COLUMN {kept}");
        sqlx::query(sqlx::AssertSqlSafe(statement))
            .execute(&mut *tx)
            .await
            .unwrap();
        let found = key_columns(&mut tx).await;
        let column = kept.split(' ').next().unwrap();
        assert_eq!(found, [format!("checkpoints.{column}")]);
        tx.rollback().await.unwrap();
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}
