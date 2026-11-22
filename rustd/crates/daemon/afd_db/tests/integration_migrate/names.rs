//! Table and key names follow the placement and foreign-key
//! rules: no table repeats its schema's name as a prefix, and every foreign-key
//! column ends `_id` and names the table it points at, or a stated role.

use super::*;

/// Every product table.
const TABLES: &str = "\
SELECT table_schema::text, table_name::text
  FROM information_schema.tables
 WHERE table_type = 'BASE TABLE'
   AND table_schema NOT IN ('pg_catalog', 'information_schema', 'audit')";

/// Every foreign-key column, the table it references, and the column there.
const FOREIGN_KEY_COLUMNS: &str = "\
SELECT src.relname::text, src_col.attname::text, dst.relname::text, dst_col.attname::text, cardinality(con.conkey)::int4
  FROM pg_constraint con
  JOIN pg_class src ON src.oid = con.conrelid
  JOIN pg_class dst ON dst.oid = con.confrelid
  JOIN LATERAL unnest(con.conkey, con.confkey) AS k(src_attnum, dst_attnum) ON true
  JOIN pg_attribute src_col ON src_col.attrelid = con.conrelid AND src_col.attnum = k.src_attnum
  JOIN pg_attribute dst_col ON dst_col.attrelid = con.confrelid AND dst_col.attnum = k.dst_attnum
 WHERE con.contype = 'f'";

/// Pointers whose role name is not their table's name, each stated in its slot.
const ROLE_NAMED: [(&str, &str); 1] = [
    // An approval that activated a grant: the gate names the grant it made live.
    ("active_grant_id", "integration_grants"),
];

/// The singular a pointer column is named for: `deliveries` → `delivery`.
fn singular(table: &str) -> String {
    table.strip_suffix("ies").map_or_else(
        || table.strip_suffix('s').unwrap_or(table).to_owned(),
        |stem| format!("{stem}y"),
    )
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_table_and_key_names_follow_rules() {
    let (database, db) = migrated().await;
    let mut connection = db.acquire().await.unwrap();

    let tables: Vec<(String, String)> = sqlx::query_as(TABLES)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    for (schema, table) in &tables {
        assert!(
            !table.starts_with(&format!("{schema}_")),
            "{schema}.{table} repeats its schema's name"
        );
    }

    let keys: Vec<(String, String, String, String, i32)> = sqlx::query_as(FOREIGN_KEY_COLUMNS)
        .fetch_all(&mut *connection)
        .await
        .unwrap();
    assert!(!keys.is_empty(), "the catalog read found no foreign keys");
    for (table, column, target, target_column, arity) in &keys {
        // A composite reference repeats the target's own column name for every
        // part but the pointer (`tenant_id` beside `tenant_model_id`).
        let named = key_names_match(table, column, target, target_column, *arity);
        assert!(
            named,
            "{table}.{column} points at {target}.{target_column} without naming it"
        );
    }

    drop(connection);
    db.close().await;
    database.cleanup().await;
}

#[test]
fn test_singular_reads_the_pointer_name_a_table_takes() {
    assert_eq!(singular("deliveries"), "delivery");
    assert_eq!(singular("revisions"), "revision");
    assert_eq!(singular("threads"), "thread");
}

/// Scope dimensions repeat their target only inside a composite reference.
/// The two primary-id roles below close the task/checkpoint and turn/checkpoint
/// ownership cycles, and a turn's thread stands for the fleet that owns it.
fn key_names_match(
    table: &str,
    column: &str,
    target: &str,
    target_column: &str,
    arity: i32,
) -> bool {
    if target_column == "id" {
        column.ends_with("_id")
            && (column.ends_with(&format!("{}_id", singular(target)))
                || ROLE_NAMED.contains(&(column, target)))
    } else {
        arity > 1
            && (column == target_column
                    || (table == "durable_tasks"
                        && column == "id"
                        && target == "durable_task_checkpoints"
                        && target_column == "durable_task_id")
                    // A turn's recovery head proves the checkpoint is the turn's own.
                    || (table == "turns"
                        && column == "id"
                        && target == "turn_checkpoints"
                        && target_column == "turn_id")
                    // A fleet's thread is keyed by the fleet's id, so a turn
                    // scopes its task by `thread_id`.
                    || (table == "turns" && column == "thread_id" && target_column == "fleet_id"))
    }
}

#[test]
fn composite_scope_columns_preserve_target_identity() {
    assert!(key_names_match(
        "durable_task_checkpoints",
        "durable_task_base_id",
        "durable_task_bases",
        "id",
        1
    ));
    assert!(key_names_match(
        "durable_task_checkpoints",
        "repository",
        "durable_task_bases",
        "repository",
        3
    ));
    assert!(key_names_match(
        "durable_tasks",
        "id",
        "durable_task_checkpoints",
        "durable_task_id",
        4
    ));
    assert!(!key_names_match(
        "durable_task_checkpoints",
        "base_id",
        "durable_task_bases",
        "id",
        1
    ));
    assert!(!key_names_match(
        "durable_task_checkpoints",
        "wrong_repository",
        "durable_task_bases",
        "repository",
        3
    ));
    assert!(!key_names_match(
        "other_durable_tasks",
        "id",
        "durable_task_checkpoints",
        "durable_task_id",
        4
    ));
    assert!(!key_names_match(
        "durable_tasks",
        "id",
        "durable_task_checkpoints",
        "other_durable_task_id",
        4
    ));
    assert!(key_names_match(
        "turns",
        "thread_id",
        "durable_tasks",
        "fleet_id",
        3
    ));
    assert!(!key_names_match(
        "turns",
        "thread_id",
        "durable_tasks",
        "fleet_id",
        1
    ));
    assert!(!key_names_match("members", "email", "users", "email", 1));
    assert!(!key_names_match(
        "durable_tasks",
        "id",
        "durable_task_checkpoints",
        "durable_task_id",
        1
    ));
}
